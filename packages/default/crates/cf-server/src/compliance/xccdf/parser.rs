//! Secure XCCDF 1.2 and CF-XCCDF extension parser.
//!
//! Uses `quick-xml`'s [`NsReader`] so elements are matched by resolved
//! namespace URI, never by literal prefix. DTD and entity processing are
//! disabled. Accepts raw bytes from file upload or ZIP extraction, classifies
//! documents, and returns typed structures for preview and import.

use anyhow::Result;
use quick_xml::events::{BytesStart, Event};
use quick_xml::name::ResolveResult;
use quick_xml::reader::NsReader;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::io::Cursor;
use uuid::Uuid;

use super::super::interchange::{
    InterchangeLimits, CF_XCCDF_NAMESPACE, XCCDF_1_1_NAMESPACE, XCCDF_NAMESPACE,
};
use super::models::*;

// ── Namespace classification ──────────────────────────────────────────────────

/// Resolved namespace of an element name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ElementNamespace {
    /// `http://checklists.nist.gov/xccdf/1.2`
    Xccdf,
    /// `urn:crystal-forge:xccdf:1`
    CrystalForge,
    /// No namespace binding (no prefix, no default namespace).
    Unbound,
    /// Bound to some other namespace.
    Other,
}

/// Map a resolved element name to a known namespace.
///
/// An undeclared prefix is a blocking error: a document that references an
/// unresolvable namespace is ambiguous and must not be classified silently.
fn classify_element_namespace(
    resolved: &ResolveResult<'_>,
) -> Result<ElementNamespace, Diagnostic> {
    match resolved {
        // XCCDF 1.1.4 and XCCDF 1.2 are both treated as the XCCDF namespace
        // for parsing purposes. CF always exports 1.2.
        ResolveResult::Bound(namespace)
            if namespace.as_ref() == XCCDF_NAMESPACE.as_bytes()
                || namespace.as_ref() == XCCDF_1_1_NAMESPACE.as_bytes() =>
        {
            Ok(ElementNamespace::Xccdf)
        }
        ResolveResult::Bound(namespace) if namespace.as_ref() == CF_XCCDF_NAMESPACE.as_bytes() => {
            Ok(ElementNamespace::CrystalForge)
        }
        ResolveResult::Bound(_) => Ok(ElementNamespace::Other),
        ResolveResult::Unbound => Ok(ElementNamespace::Unbound),
        ResolveResult::Unknown(prefix) => Err(Diagnostic::error(
            "XML_UNKNOWN_NAMESPACE_PREFIX",
            &format!(
                "Element uses an undeclared namespace prefix: {}",
                String::from_utf8_lossy(prefix)
            ),
        )),
    }
}

/// Extract the source XCCDF namespace version string from a resolved namespace,
/// returning `"1.1"` or `"1.2"` (or `None` for non-XCCDF namespaces).
fn xccdf_namespace_version(resolved: &ResolveResult<'_>) -> Option<&'static str> {
    if let ResolveResult::Bound(ns) = resolved {
        if ns.as_ref() == XCCDF_NAMESPACE.as_bytes() {
            return Some("1.2");
        }
        if ns.as_ref() == XCCDF_1_1_NAMESPACE.as_bytes() {
            return Some("1.1");
        }
    }
    None
}

/// Flow control for parser handlers: `Abort` stops the outer event loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ParseControl {
    Continue,
    Abort,
}

// ── Attribute parsing ─────────────────────────────────────────────────────────

/// Parse all attributes of an element in a single pass, enforcing the per-element
/// attribute limit and detecting both malformed attributes and duplicates.
///
/// Attribute values are unescaped (entity references resolved). The returned
/// map is keyed by the raw qualified attribute name bytes (e.g. `b"id"`,
/// `b"xccdf:version"`).
fn parse_attributes(
    element: &BytesStart<'_>,
    limit: usize,
) -> Result<HashMap<Vec<u8>, String>, Diagnostic> {
    let mut parsed: HashMap<Vec<u8>, String> = HashMap::new();

    for attribute in element.attributes() {
        let attribute = attribute.map_err(|error| {
            Diagnostic::error(
                "XML_ATTRIBUTE_ERROR",
                &format!("Invalid XML attribute: {error}"),
            )
        })?;

        if parsed.len() >= limit {
            return Err(Diagnostic::error(
                "ATTRIBUTE_LIMIT_EXCEEDED",
                &format!("Element exceeds the maximum of {limit} attributes"),
            ));
        }

        let key = attribute.key.as_ref().to_vec();

        if parsed.contains_key(&key) {
            return Err(Diagnostic::error(
                "DUPLICATE_ATTRIBUTE",
                &format!("Duplicate attribute '{}'", String::from_utf8_lossy(&key)),
            ));
        }

        let value = attribute
            .unescape_value()
            .map_err(|error| {
                Diagnostic::error(
                    "XML_ATTRIBUTE_ERROR",
                    &format!("Invalid XML attribute value: {error}"),
                )
            })?
            .into_owned();

        parsed.insert(key, value);
    }

    Ok(parsed)
}

/// Look up an attribute value by its raw qualified name, returning a `&str`
/// when present.
fn attr<'a>(attrs: &'a HashMap<Vec<u8>, String>, key: &[u8]) -> Option<&'a str> {
    attrs.get(key).map(String::as_str)
}

/// Parse an XSD boolean lexical value from an attribute string.
///
/// Accepted values: `"true"`, `"1"`, `"false"`, `"0"`.
/// Returns `None` for unrecognized values; the caller records a blocking
/// diagnostic rather than silently coercing the value to `false`.
fn parse_xsd_boolean(s: &str) -> Option<bool> {
    match s {
        "true" | "1" => Some(true),
        "false" | "0" => Some(false),
        _ => None,
    }
}

fn parse_check_boolean(
    attrs: &HashMap<Vec<u8>, String>,
    key: &[u8],
    errors: &mut Vec<Diagnostic>,
) -> Option<bool> {
    let value = attr(attrs, key)?;
    match parse_xsd_boolean(value) {
        Some(parsed) => Some(parsed),
        None => {
            errors.push(Diagnostic::error(
                "INVALID_XSD_BOOLEAN",
                &format!(
                    "{} must be one of true, 1, false, or 0; got {:?}",
                    String::from_utf8_lossy(key),
                    value
                ),
            ));
            None
        }
    }
}

// ── Entry point ───────────────────────────────────────────────────────────────

/// Parse a raw byte slice as XCCDF/XML with security limits applied.
pub fn parse_xccdf(
    bytes: &[u8],
    filename: Option<&str>,
    limits: &InterchangeLimits,
) -> Result<ParsedXccdf> {
    limits.check_xml_size(bytes.len())?;

    let sha256 = hex::encode(Sha256::digest(bytes));

    // Configure reader: no DTD, no entities, no expansion.
    let mut reader = NsReader::from_reader(Cursor::new(bytes));
    reader.config_mut().trim_text(true);

    let mut state = ParserState::new(limits);
    state.filename = filename.map(String::from);
    state.source_bytes = bytes.to_vec();
    state.source_sha256 = sha256.clone();

    let mut buffer = Vec::new();
    loop {
        let (resolved, event) = match reader.read_resolved_event_into(&mut buffer) {
            Ok(value) => value,
            Err(error) => {
                state.errors.push(Diagnostic {
                    code: "XML_PARSE_ERROR".into(),
                    summary: format!("XML parse error: {error}"),
                    field: None,
                    xml_line: None,
                    xml_column: None,
                    object_identity: None,
                    blocking: true,
                    remediation: Some("Verify the XML document is well-formed.".into()),
                });
                break;
            }
        };

        match event {
            Event::Start(element) => {
                let namespace = match classify_element_namespace(&resolved) {
                    Ok(namespace) => namespace,
                    Err(diagnostic) => {
                        state.errors.push(diagnostic);
                        break;
                    }
                };
                state.depth += 1;
                if state.depth > limits.max_xml_depth as u64 {
                    state.errors.push(Diagnostic::error(
                        "XML_DEPTH_EXCEEDED",
                        &format!(
                            "XML depth {} exceeds maximum {}",
                            state.depth, limits.max_xml_depth
                        ),
                    ));
                    break;
                }
                let local_name = element.local_name();
                // Record the source XCCDF namespace version from the first
                // Benchmark element so the preview can report it.
                if state.detected_xccdf_version.is_none()
                    && namespace == ElementNamespace::Xccdf
                    && local_name.as_ref() == b"Benchmark"
                {
                    state.detected_xccdf_version = xccdf_namespace_version(&resolved);
                }
                if state.handle_start(namespace, local_name.as_ref(), &element)
                    == ParseControl::Abort
                {
                    break;
                }
            }
            Event::Empty(element) => {
                let namespace = match classify_element_namespace(&resolved) {
                    Ok(namespace) => namespace,
                    Err(diagnostic) => {
                        state.errors.push(diagnostic);
                        break;
                    }
                };
                let local_name = element.local_name();
                if state.handle_start(namespace, local_name.as_ref(), &element)
                    == ParseControl::Abort
                {
                    break;
                }
                if state.handle_end(namespace, local_name.as_ref()) == ParseControl::Abort {
                    break;
                }
            }
            Event::End(element) => {
                let namespace = match classify_element_namespace(&resolved) {
                    Ok(namespace) => namespace,
                    Err(diagnostic) => {
                        state.errors.push(diagnostic);
                        break;
                    }
                };
                state.depth = state.depth.saturating_sub(1);
                let local_name = element.local_name();
                if state.handle_end(namespace, local_name.as_ref()) == ParseControl::Abort {
                    break;
                }
            }
            Event::Text(text) => match text.unescape() {
                Ok(text) => {
                    if state.handle_text(&text) == ParseControl::Abort {
                        break;
                    }
                }
                Err(error) => {
                    state.errors.push(Diagnostic::error(
                        "XML_ENTITY_ERROR",
                        &format!("Invalid XML entity reference: {error}"),
                    ));
                    break;
                }
            },
            Event::CData(text) => {
                let bytes: &[u8] = &text;
                if let Ok(text) = std::str::from_utf8(bytes) {
                    if state.handle_text(text) == ParseControl::Abort {
                        break;
                    }
                }
            }
            Event::DocType(_) => {
                state.errors.push(Diagnostic::error(
                    "DTD_FORBIDDEN",
                    "DOCTYPE declarations and DTDs are not accepted",
                ));
                break;
            }
            Event::Eof => break,
            _ => {}
        }

        buffer.clear();
    }

    // Classification.
    classify(&mut state);

    Ok(ParsedXccdf {
        class: state.class,
        fidelity: state.fidelity,
        fidelity_losses: state.fidelity_losses,
        source_filename: state.filename,
        source_bytes: state.source_bytes,
        source_sha256: state.source_sha256,
        xccdf_namespace_version: state.detected_xccdf_version,
        xccdf_version: state.xccdf_version.clone(),
        benchmark: state.benchmark.take(),
        profiles: state.profiles,
        rules: state.rules,
        groups: state.groups,
        values: state.values,
        cf_bundle_meta: state.cf_bundle_meta.take(),
        signature_info: state.signature_info.take(),
        errors: state.errors,
        warnings: state.warnings,
    })
}

// ── Parser state machine ──────────────────────────────────────────────────────

struct ParserState {
    limits: InterchangeLimits,
    depth: u64,
    filename: Option<String>,
    source_bytes: Vec<u8>,
    source_sha256: String,
    class: DocumentClass,
    fidelity: Fidelity,
    fidelity_losses: Vec<String>,
    xccdf_version: Option<String>,

    benchmark: Option<BenchmarkMeta>,
    profiles: Vec<ParsedProfile>,
    rules: Vec<ParsedRule>,
    groups: Vec<ParsedGroup>,
    values: Vec<ParsedValue>,
    cf_bundle_meta: Option<CfBundleMeta>,
    signature_info: Option<SignatureInfo>,
    errors: Vec<Diagnostic>,
    warnings: Vec<Diagnostic>,

    // Bounded identity sets for duplicate-ID detection. Each set is bounded by
    // the corresponding count limit, which is enforced before insertion.
    benchmark_ids: HashSet<String>,
    profile_ids: HashSet<String>,
    rule_ids: HashSet<String>,
    group_ids: HashSet<String>,
    value_ids: HashSet<String>,

    // CF classification tracking.
    /// True when at least one recognised CF extension element was parsed.
    saw_supported_cf_content: bool,
    /// True when a CF-namespace element with an unrecognised local name was
    /// encountered.  Triggers `CfNativeUnsupportedExtension`.
    saw_unknown_cf_content: bool,

    /// Source XCCDF namespace version detected from the first Benchmark element:
    /// `Some("1.1")` or `Some("1.2")`.
    detected_xccdf_version: Option<&'static str>,

    // Parsing state.
    current_text: String,
    current_rule: Option<ParsedRule>,
    current_profile: Option<ParsedProfile>,
    current_group: Option<ParsedGroup>,
    current_check: Option<PendingCheck>,
    current_fix: Option<FixContent>,
    current_ident: Option<StandardIdentifier>,
    current_ref: Option<Reference>,
}

struct PendingCheck {
    system: String,
    selector: Option<String>,
    multi_check: Option<bool>,
    negate: Option<bool>,
    body_parts: Vec<CheckBodyPart>,
    has_inline: bool,
    body_error: bool,
}

impl ParserState {
    fn new(limits: &InterchangeLimits) -> Self {
        Self {
            limits: limits.clone(),
            depth: 0,
            filename: None,
            source_bytes: vec![],
            source_sha256: String::new(),
            class: DocumentClass::InvalidXccdf,
            fidelity: Fidelity::Degraded,
            fidelity_losses: vec![],
            xccdf_version: None,
            benchmark: None,
            profiles: vec![],
            rules: vec![],
            groups: vec![],
            values: vec![],
            cf_bundle_meta: None,
            signature_info: None,
            errors: vec![],
            warnings: vec![],
            benchmark_ids: HashSet::new(),
            profile_ids: HashSet::new(),
            rule_ids: HashSet::new(),
            group_ids: HashSet::new(),
            value_ids: HashSet::new(),
            saw_supported_cf_content: false,
            saw_unknown_cf_content: false,
            detected_xccdf_version: None,
            current_text: String::new(),
            current_rule: None,
            current_profile: None,
            current_group: None,
            current_check: None,
            current_fix: None,
            current_ident: None,
            current_ref: None,
        }
    }

    fn handle_start(
        &mut self,
        namespace: ElementNamespace,
        local_name: &[u8],
        e: &quick_xml::events::BytesStart,
    ) -> ParseControl {
        self.current_text.clear();

        // Parse attributes in one pass: enforces the per-element limit,
        // detects malformed and duplicate attributes, and unescapes values.
        let attrs = match parse_attributes(e, self.limits.max_attributes_per_element) {
            Ok(a) => a,
            Err(diag) => {
                self.errors.push(diag);
                return ParseControl::Abort;
            }
        };

        // Warn when the root element looks like XCCDF but is not bound to the
        // XCCDF namespace.
        if self.depth == 1 && local_name == b"Benchmark" && namespace != ElementNamespace::Xccdf {
            self.warnings.push(Diagnostic::warning(
                "UNKNOWN_NAMESPACE",
                &format!(
                    "Root Benchmark element is not bound to the XCCDF namespace '{}'",
                    XCCDF_NAMESPACE
                ),
            ));
        }

        match (namespace, local_name) {
            // ── XCCDF structure ────────────────────────────────────────────
            (ElementNamespace::Xccdf, b"Benchmark") => self.parse_benchmark_start(&attrs),
            (ElementNamespace::Xccdf, b"Profile") => self.parse_profile_start(&attrs),
            (ElementNamespace::Xccdf, b"Rule") => self.parse_rule_start(&attrs),
            (ElementNamespace::Xccdf, b"Group") => self.parse_group_start(&attrs),
            (ElementNamespace::Xccdf, b"Value") => self.parse_value_start(&attrs),
            (ElementNamespace::Xccdf, b"status") => {
                if let Some(d) = attr(&attrs, b"date") {
                    if let Some(ref mut bm) = self.benchmark {
                        bm.status_date = Some(d.to_string());
                    }
                }
                ParseControl::Continue
            }
            (ElementNamespace::Xccdf, b"platform") => {
                if let Some(v) = attr(&attrs, b"idref") {
                    if let Some(ref mut bm) = self.benchmark {
                        bm.platforms.push(v.to_string());
                    }
                }
                ParseControl::Continue
            }
            (ElementNamespace::Xccdf, b"reference") => {
                let href = attr(&attrs, b"href").map(String::from);
                self.current_ref = Some(Reference { href, title: None });
                ParseControl::Continue
            }
            (ElementNamespace::Xccdf, b"select") => {
                if let Some(v) = attr(&attrs, b"idref") {
                    if let Some(ref mut pr) = self.current_profile {
                        pr.select_ids.push(v.to_string());
                    }
                }
                ParseControl::Continue
            }
            (ElementNamespace::Xccdf, b"check") => {
                self.parse_check_start(&attrs);
                ParseControl::Continue
            }
            (ElementNamespace::Xccdf, b"check-content-ref") => {
                if let Some(ref mut check) = self.current_check {
                    let Some(href) = attr(&attrs, b"href").filter(|value| !value.is_empty()) else {
                        self.errors.push(Diagnostic::error(
                            "CHECK_REF_MISSING_HREF",
                            "check-content-ref requires a non-empty href attribute",
                        ));
                        check.body_error = true;
                        return ParseControl::Abort;
                    };
                    // XCCDF 1.1 §7.7.2.5 / XCCDF 1.2 §7.7.3.4:
                    //   A <check> MAY contain zero-or-more <check-content-ref> followed by
                    //   zero-or-one <check-content>.  References appear before any inline
                    //   fallback.  A reference after an inline body violates the schema
                    //   sequence and is rejected.
                    if check.has_inline {
                        self.errors.push(Diagnostic::error(
                            "CHECK_REFERENCE_AFTER_INLINE",
                            "check-content-ref must appear before any inline check-content element",
                        ));
                        check.body_error = true;
                    }
                    check.body_parts.push(CheckBodyPart::Reference {
                        href: href.to_owned(),
                        name: attr(&attrs, b"name").map(str::to_owned),
                    });
                }
                ParseControl::Continue
            }
            (ElementNamespace::Xccdf, b"fix") => {
                self.parse_fix_start(&attrs);
                ParseControl::Continue
            }
            (ElementNamespace::Xccdf, b"fixtext") => {
                // <fixtext fixref="F-..."> contains the human-readable remediation.
                // XCCDF 1.1/1.2 STIGs typically use <fixtext> rather than <fix>
                // for the prose remediation. Treat fixref as the id so it can be
                // correlated with the companion empty <fix id="..."/> element.
                let id = attr(&attrs, b"fixref").map(String::from);
                // Only initialize if we don't already have a <fix> in progress;
                // that preserves priority for a genuine script-bearing <fix>.
                if self.current_fix.is_none() {
                    self.current_fix = Some(FixContent {
                        id,
                        system: None,
                        content: String::new(),
                        complexity: None,
                        disruption: None,
                    });
                }
                ParseControl::Continue
            }
            (ElementNamespace::Xccdf, b"ident") => {
                let system = attr(&attrs, b"system").unwrap_or("").to_string();
                self.current_ident = Some(StandardIdentifier {
                    system,
                    value: String::new(),
                });
                ParseControl::Continue
            }
            // ── Crystal Forge extension elements ───────────────────────────
            (ElementNamespace::CrystalForge, b"bundle") => {
                self.saw_supported_cf_content = true;
                self.parse_cf_bundle_start(&attrs);
                ParseControl::Continue
            }
            (ElementNamespace::CrystalForge, b"policy-identity") => {
                self.saw_supported_cf_content = true;
                self.parse_cf_policy_identity_start(&attrs);
                ParseControl::Continue
            }
            (ElementNamespace::CrystalForge, b"execution") => {
                self.saw_supported_cf_content = true;
                if let Some(ref mut meta) = self
                    .current_rule
                    .as_mut()
                    .and_then(|r| r.cf_policy_meta.as_mut())
                {
                    meta.execution_phase = attr(&attrs, b"phase").map(String::from);
                    meta.strict = attr(&attrs, b"strict").map(|s| s == "true");
                }
                ParseControl::Continue
            }
            (ElementNamespace::CrystalForge, b"implementation") => {
                self.saw_supported_cf_content = true;
                if let Some(meta) = self
                    .current_rule
                    .as_mut()
                    .and_then(|r| r.cf_policy_meta.as_mut())
                {
                    meta.implementation_state = attr(&attrs, b"state").map(str::to_owned);
                }
                ParseControl::Continue
            }
            (ElementNamespace::CrystalForge, b"config-json")
            | (ElementNamespace::CrystalForge, b"compliance-metadata-json")
            | (ElementNamespace::CrystalForge, b"dependencies-json") => {
                self.saw_supported_cf_content = true;
                ParseControl::Continue
            }
            (ElementNamespace::CrystalForge, implementation)
                if matches!(
                    implementation,
                    b"require-crystal-forge-agent"
                        | b"require-packages"
                        | b"custom-check"
                        | b"require-cve-check"
                        | b"time-window"
                        | b"require-approvals"
                        | b"canary-rollout"
                        | b"cve-threshold"
                        | b"unsupported"
                ) =>
            {
                self.saw_supported_cf_content = true;
                if let Some(meta) = self
                    .current_rule
                    .as_mut()
                    .and_then(|r| r.cf_policy_meta.as_mut())
                {
                    if meta.policy_type.is_none() {
                        meta.policy_type =
                            Some(if implementation == b"require-crystal-forge-agent" {
                                "require_cf_agent".into()
                            } else {
                                String::from_utf8_lossy(implementation).replace('-', "_")
                            });
                    }
                }
                ParseControl::Continue
            }
            // Recognised CF text-only elements (content captured in handle_end).
            (ElementNamespace::CrystalForge, b"content-digest") => {
                let policy_digest = if let Some(meta) = self
                    .current_rule
                    .as_mut()
                    .and_then(|r| r.cf_policy_meta.as_mut())
                {
                    meta.digest_algorithm = attr(&attrs, b"algorithm").map(str::to_owned);
                    meta.canonicalization_version =
                        attr(&attrs, b"canonical-model").map(str::to_owned);
                    true
                } else {
                    false
                };
                if !policy_digest {
                    if let Some(meta) = self.cf_bundle_meta.as_mut() {
                        meta.digest_algorithm = attr(&attrs, b"algorithm").map(str::to_owned);
                        meta.canonicalization_version =
                            attr(&attrs, b"canonical-model").map(str::to_owned);
                    }
                }
                self.saw_supported_cf_content = true;
                ParseControl::Continue
            }
            (
                ElementNamespace::CrystalForge,
                b"policy" | b"framework" | b"layer" | b"owner" | b"policy-version",
            ) => {
                self.saw_supported_cf_content = true;
                if local_name == b"policy" {
                    if let Some(meta) = self
                        .current_rule
                        .as_mut()
                        .and_then(|r| r.cf_policy_meta.as_mut())
                    {
                        meta.policy_type = attr(&attrs, b"policy-type").map(str::to_owned);
                    }
                }
                if local_name == b"framework" {
                    if let Some(meta) = self.cf_bundle_meta.as_mut() {
                        meta.framework = attr(&attrs, b"name").map(str::to_owned);
                        meta.framework_version = attr(&attrs, b"version").map(str::to_owned);
                    }
                }
                ParseControl::Continue
            }
            // Unknown CF-namespace element: marks the document as using an
            // unsupported CF extension so classification is downgraded.
            (ElementNamespace::CrystalForge, _) => {
                self.saw_unknown_cf_content = true;
                ParseControl::Continue
            }
            // Text-bearing XCCDF elements and everything else are consumed by
            // the end handler or ignored. Unknown content never reclassifies
            // an element: only its resolved namespace decides.
            _ => ParseControl::Continue,
        }
    }

    fn handle_end(&mut self, namespace: ElementNamespace, local_name: &[u8]) -> ParseControl {
        match (namespace, local_name) {
            (ElementNamespace::Xccdf, b"title") => {
                if let Some(ref mut bm) = self.benchmark {
                    bm.title = bm.title.clone().or(Some(self.current_text.clone()));
                }
                if let Some(ref mut pr) = self.current_profile {
                    pr.title = Some(self.current_text.clone());
                }
                if let Some(ref mut rule) = self.current_rule {
                    rule.title = Some(self.current_text.clone());
                }
                if let Some(ref mut grp) = self.current_group {
                    grp.title = Some(self.current_text.clone());
                }
            }
            (ElementNamespace::Xccdf, b"description") => {
                if let Some(ref mut bm) = self.benchmark {
                    bm.description = bm.description.clone().or(Some(self.current_text.clone()));
                }
                if let Some(ref mut rule) = self.current_rule {
                    rule.description = Some(self.current_text.clone());
                }
            }
            (ElementNamespace::Xccdf, b"version") => {
                if let Some(ref mut bm) = self.benchmark {
                    bm.version = Some(self.current_text.clone());
                }
            }
            (ElementNamespace::Xccdf, b"status") => {
                if let Some(ref mut bm) = self.benchmark {
                    bm.status = Some(self.current_text.clone());
                }
            }
            (ElementNamespace::Xccdf, b"reference") => {
                if let Some(ref mut r) = self.current_ref {
                    r.title = Some(self.current_text.clone());
                }
                let done = self.current_ref.take();
                if let Some(ref mut rule) = self.current_rule {
                    if let Some(r) = done {
                        rule.references.push(r);
                    }
                }
            }
            (ElementNamespace::Xccdf, b"Profile") => {
                if let Some(pr) = self.current_profile.take() {
                    self.profiles.push(pr);
                }
            }
            (ElementNamespace::Xccdf, b"Rule") => {
                if let Some(rule) = self.current_rule.take() {
                    self.rules.push(rule);
                }
            }
            (ElementNamespace::Xccdf, b"Group") => {
                if let Some(grp) = self.current_group.take() {
                    self.groups.push(grp);
                }
            }
            (ElementNamespace::Xccdf, b"check") => {
                if let Some(check) = self.current_check.take() {
                    if check.body_error {
                        // A diagnostic was already emitted for the invalid
                        // body combination; do not construct a lossy check.
                    } else if check.body_parts.is_empty() {
                        self.errors.push(Diagnostic::error(
                            "CHECK_BODY_MISSING",
                            "check must contain at least one check-content-ref or check-content body",
                        ));
                    } else if let Some(ref mut rule) = self.current_rule {
                        rule.checks.push(CheckContent {
                            system: check.system,
                            body_parts: check.body_parts,
                            selector: check.selector,
                            multi_check: check.multi_check,
                            negate: check.negate,
                        });
                    }
                }
            }
            (ElementNamespace::Xccdf, b"check-content") => {
                if let Some(ref mut check) = self.current_check {
                    if check.has_inline {
                        // XCCDF allows at most one inline check-content per check.
                        self.errors.push(Diagnostic::error(
                            "CHECK_INLINE_DUPLICATE",
                            "check contains more than one inline check-content element",
                        ));
                        check.body_error = true;
                    } else {
                        // XCCDF 1.1 §7.7.2.5: zero-or-more check-content-ref followed by
                        // zero-or-one check-content.  An inline fallback after references is
                        // valid and is the pattern used by official DISA STIGs that reference
                        // external OVAL/STIG data but embed a manual check procedure.
                        check.has_inline = true;
                        check.body_parts.push(CheckBodyPart::Inline {
                            content: self.current_text.clone(),
                        });
                    }
                }
            }
            (ElementNamespace::Xccdf, b"fix") => {
                if let Some(ref mut fix) = self.current_fix {
                    fix.content = self.current_text.clone();
                }
                let fix = self.current_fix.take();
                if let Some(ref mut rule) = self.current_rule {
                    // Only replace an existing fix (from <fixtext>) when this <fix>
                    // element actually carries content. This prevents an empty
                    // <fix id="..."/> companion element from clearing the prose
                    // already extracted from <fixtext>.
                    match (&rule.fix, &fix) {
                        (Some(_), Some(f)) if f.content.is_empty() => {
                            // Empty <fix> — keep the existing fixtext content.
                        }
                        _ => rule.fix = fix,
                    }
                }
            }
            (ElementNamespace::Xccdf, b"fixtext") => {
                if let Some(ref mut fix) = self.current_fix {
                    fix.content = self.current_text.clone();
                }
                // Commit fixtext to rule.fix when no <fix> element has done so yet.
                // A subsequent empty <fix id="..."/> will overwrite only if it has
                // actual content (checked in the </fix> handler).
                let fix = self.current_fix.take();
                if let Some(ref mut rule) = self.current_rule {
                    if rule.fix.is_none() {
                        rule.fix = fix;
                    } else {
                        // Already have a fix from a <fix> element — restore current_fix
                        // so the </fix> handler can still reach it if needed.
                        self.current_fix = fix;
                    }
                }
            }
            (ElementNamespace::Xccdf, b"ident") => {
                let ident = self.current_ident.take();
                if let Some(ref mut rule) = self.current_rule {
                    if let Some(i) = ident {
                        rule.identifiers.push(i);
                    }
                }
            }
            (ElementNamespace::Xccdf, b"rationale") => {
                if let Some(ref mut rule) = self.current_rule {
                    rule.rationale = Some(self.current_text.clone());
                }
            }
            // ── Crystal Forge extension text elements ──────────────────────
            (ElementNamespace::CrystalForge, b"framework") => {
                if let Some(ref mut meta) = self.cf_bundle_meta {
                    if !self.current_text.is_empty() {
                        meta.framework = Some(self.current_text.clone());
                    }
                }
            }
            (ElementNamespace::CrystalForge, b"layer") => {
                if let Some(ref mut meta) = self.cf_bundle_meta {
                    meta.layer = Some(self.current_text.clone());
                }
            }
            (ElementNamespace::CrystalForge, b"owner") => {
                if let Some(ref mut meta) = self.cf_bundle_meta {
                    meta.owner = Some(self.current_text.clone());
                }
            }
            (ElementNamespace::CrystalForge, b"content-digest") => {
                let policy_digest = if let Some(ref mut meta) = self
                    .current_rule
                    .as_mut()
                    .and_then(|r| r.cf_policy_meta.as_mut())
                {
                    meta.digest = Some(self.current_text.clone());
                    true
                } else {
                    false
                };
                if !policy_digest {
                    if let Some(ref mut meta) = self.cf_bundle_meta {
                        meta.digest = Some(self.current_text.clone());
                    }
                }
            }
            (ElementNamespace::CrystalForge, b"policy-version") => {
                if let Some(ref mut meta) = self
                    .current_rule
                    .as_mut()
                    .and_then(|r| r.cf_policy_meta.as_mut())
                {
                    meta.version = Some(self.current_text.clone());
                }
            }
            (ElementNamespace::CrystalForge, b"config-json") => {
                if let Some(meta) = self
                    .current_rule
                    .as_mut()
                    .and_then(|r| r.cf_policy_meta.as_mut())
                {
                    meta.config = serde_json::from_str(&self.current_text).ok();
                }
            }
            (ElementNamespace::CrystalForge, b"compliance-metadata-json") => {
                if let Some(meta) = self
                    .current_rule
                    .as_mut()
                    .and_then(|r| r.cf_policy_meta.as_mut())
                {
                    meta.compliance_metadata = serde_json::from_str(&self.current_text).ok();
                }
            }
            (ElementNamespace::CrystalForge, b"dependencies-json") => {
                if let Some(meta) = self
                    .current_rule
                    .as_mut()
                    .and_then(|r| r.cf_policy_meta.as_mut())
                {
                    meta.dependencies = serde_json::from_str(&self.current_text).ok();
                }
            }
            _ => {}
        }

        ParseControl::Continue
    }

    fn handle_text(&mut self, text: &str) -> ParseControl {
        if self.current_text.len() + text.len() > self.limits.max_text_node_bytes {
            self.errors.push(Diagnostic::error(
                "TEXT_TOO_LARGE",
                &format!(
                    "Cumulative text {} exceeds maximum of {} bytes",
                    self.current_text.len() + text.len(),
                    self.limits.max_text_node_bytes
                ),
            ));
            return ParseControl::Abort;
        }
        self.current_text.push_str(text);

        // Pass ident text.
        if let Some(ref mut ident) = self.current_ident {
            ident.value = self.current_text.clone();
        }
        ParseControl::Continue
    }

    // ── Limit guards ──────────────────────────────────────────────────────────
    //
    // Each guard runs before a new object is allocated. A violation records one
    // blocking diagnostic and aborts parsing; no further objects are collected.

    fn begin_rule(&mut self) -> ParseControl {
        if self.rules.len() >= self.limits.max_rule_count {
            self.errors.push(Diagnostic::error(
                "RULE_LIMIT_EXCEEDED",
                &format!("Rule count exceeds maximum {}", self.limits.max_rule_count),
            ));
            return ParseControl::Abort;
        }
        ParseControl::Continue
    }

    fn begin_profile(&mut self) -> ParseControl {
        if self.profiles.len() >= self.limits.max_profile_count {
            self.errors.push(Diagnostic::error(
                "PROFILE_LIMIT_EXCEEDED",
                &format!(
                    "Profile count exceeds maximum {}",
                    self.limits.max_profile_count
                ),
            ));
            return ParseControl::Abort;
        }
        ParseControl::Continue
    }

    fn begin_group(&mut self) -> ParseControl {
        if self.groups.len() >= self.limits.max_group_count {
            self.errors.push(Diagnostic::error(
                "GROUP_LIMIT_EXCEEDED",
                &format!(
                    "Group count exceeds maximum {}",
                    self.limits.max_group_count
                ),
            ));
            return ParseControl::Abort;
        }
        ParseControl::Continue
    }

    fn begin_value(&mut self) -> ParseControl {
        if self.values.len() >= self.limits.max_value_count {
            self.errors.push(Diagnostic::error(
                "VALUE_LIMIT_EXCEEDED",
                &format!(
                    "Value count exceeds maximum {}",
                    self.limits.max_value_count
                ),
            ));
            return ParseControl::Abort;
        }
        ParseControl::Continue
    }

    /// Insert a non-empty ID into `ids`; a duplicate records a blocking
    /// diagnostic and aborts parsing so ambiguous source identity is rejected
    /// before import planning.
    fn check_duplicate_id(
        ids: &mut HashSet<String>,
        id: &str,
        code: &str,
        kind: &str,
        errors: &mut Vec<Diagnostic>,
    ) -> ParseControl {
        if id.is_empty() {
            return ParseControl::Continue;
        }
        if !ids.insert(id.to_string()) {
            errors.push(Diagnostic::error(
                code,
                &format!("Duplicate {kind} id '{id}' in source document"),
            ));
            return ParseControl::Abort;
        }
        ParseControl::Continue
    }

    // ── Start-element parsing ──────────────────────────────────────────────────

    fn parse_benchmark_start(&mut self, attrs: &HashMap<Vec<u8>, String>) -> ParseControl {
        let id = attr(attrs, b"id").unwrap_or("").to_string();
        if Self::check_duplicate_id(
            &mut self.benchmark_ids,
            &id,
            "DUPLICATE_BENCHMARK_ID",
            "Benchmark",
            &mut self.errors,
        ) == ParseControl::Abort
        {
            return ParseControl::Abort;
        }
        // Informational version hint from qualified attribute name.
        self.xccdf_version = attr(attrs, b"xccdf:version").map(String::from);
        self.benchmark = Some(BenchmarkMeta {
            id,
            title: None,
            description: None,
            version: None,
            status: None,
            status_date: None,
            platforms: vec![],
            publisher: None,
            references: vec![],
        });
        ParseControl::Continue
    }

    fn parse_profile_start(&mut self, attrs: &HashMap<Vec<u8>, String>) -> ParseControl {
        if self.begin_profile() == ParseControl::Abort {
            return ParseControl::Abort;
        }
        let id = attr(attrs, b"id").unwrap_or("").to_string();
        if Self::check_duplicate_id(
            &mut self.profile_ids,
            &id,
            "DUPLICATE_PROFILE_ID",
            "Profile",
            &mut self.errors,
        ) == ParseControl::Abort
        {
            return ParseControl::Abort;
        }
        let extends = attr(attrs, b"extends").map(String::from);
        let abstract_attr = attr(attrs, b"abstract")
            .map(|v| v == "true")
            .unwrap_or(false);
        self.current_profile = Some(ParsedProfile {
            id,
            title: None,
            description: None,
            select_ids: vec![],
            extends_id: extends,
            is_abstract: abstract_attr,
            is_baseline: false,
        });
        ParseControl::Continue
    }

    fn parse_rule_start(&mut self, attrs: &HashMap<Vec<u8>, String>) -> ParseControl {
        if self.begin_rule() == ParseControl::Abort {
            return ParseControl::Abort;
        }
        let id = attr(attrs, b"id").unwrap_or("").to_string();
        if Self::check_duplicate_id(
            &mut self.rule_ids,
            &id,
            "DUPLICATE_RULE_ID",
            "Rule",
            &mut self.errors,
        ) == ParseControl::Abort
        {
            return ParseControl::Abort;
        }
        let severity = attr(attrs, b"severity").map(String::from);
        let weight = attr(attrs, b"weight").and_then(|v| v.parse::<f64>().ok());
        self.current_rule = Some(ParsedRule {
            id,
            title: None,
            description: None,
            rationale: None,
            severity,
            weight,
            version: None,
            checks: vec![],
            fix: None,
            identifiers: vec![],
            references: vec![],
            platforms: vec![],
            group_id: self.current_group.as_ref().map(|g| g.id.clone()),
            rule_order: Some(self.rules.len()),
            cf_policy_meta: None,
            preserved_xml: None,
        });
        ParseControl::Continue
    }

    fn parse_group_start(&mut self, attrs: &HashMap<Vec<u8>, String>) -> ParseControl {
        if self.begin_group() == ParseControl::Abort {
            return ParseControl::Abort;
        }
        let id = attr(attrs, b"id").unwrap_or("").to_string();
        if Self::check_duplicate_id(
            &mut self.group_ids,
            &id,
            "DUPLICATE_GROUP_ID",
            "Group",
            &mut self.errors,
        ) == ParseControl::Abort
        {
            return ParseControl::Abort;
        }
        self.current_group = Some(ParsedGroup {
            id,
            title: None,
            description: None,
            rule_ids: vec![],
        });
        ParseControl::Continue
    }

    fn parse_value_start(&mut self, attrs: &HashMap<Vec<u8>, String>) -> ParseControl {
        if self.begin_value() == ParseControl::Abort {
            return ParseControl::Abort;
        }
        let id = attr(attrs, b"id").unwrap_or("").to_string();
        if Self::check_duplicate_id(
            &mut self.value_ids,
            &id,
            "DUPLICATE_VALUE_ID",
            "Value",
            &mut self.errors,
        ) == ParseControl::Abort
        {
            return ParseControl::Abort;
        }
        let vtype = attr(attrs, b"type").unwrap_or("string").to_string();
        self.values.push(ParsedValue {
            id,
            title: None,
            description: None,
            value_type: vtype,
            default_value: None,
            allowed_values: vec![],
        });
        ParseControl::Continue
    }

    fn parse_check_start(&mut self, attrs: &HashMap<Vec<u8>, String>) {
        let system = attr(attrs, b"system").unwrap_or("").to_string();
        let selector = attr(attrs, b"selector").map(String::from);
        let multi_check = parse_check_boolean(attrs, b"multi-check", &mut self.errors);
        let negate = parse_check_boolean(attrs, b"negate", &mut self.errors);
        self.current_check = Some(PendingCheck {
            system,
            selector,
            multi_check,
            negate,
            body_parts: Vec::new(),
            has_inline: false,
            body_error: false,
        });
    }

    fn parse_fix_start(&mut self, attrs: &HashMap<Vec<u8>, String>) {
        let id = attr(attrs, b"id").map(String::from);
        let system = attr(attrs, b"system").map(String::from);
        let complexity = attr(attrs, b"complexity").map(String::from);
        let disruption = attr(attrs, b"disruption").map(String::from);
        self.current_fix = Some(FixContent {
            id,
            system,
            content: String::new(),
            complexity,
            disruption,
        });
    }

    fn parse_cf_bundle_start(&mut self, attrs: &HashMap<Vec<u8>, String>) {
        let bundle_id = attr(attrs, b"bundle-id").and_then(parse_uuid_urn);
        let bvid = attr(attrs, b"bundle-version-id").and_then(parse_uuid_urn);
        if bundle_id.is_none() {
            self.warnings.push(Diagnostic::warning(
                "INVALID_CF_IDENTITY",
                "CF bundle element is missing a valid 'bundle-id' UUID",
            ));
        }
        let state = attr(attrs, b"publication-state").unwrap_or("").to_string();
        self.cf_bundle_meta = Some(CfBundleMeta {
            bundle_id: bundle_id.unwrap_or_else(Uuid::nil),
            bundle_version_id: bvid.unwrap_or_else(Uuid::nil),
            schema_version: attr(attrs, b"schema-version").map(str::to_owned),
            publication_state: state,
            framework: None,
            framework_version: None,
            layer: None,
            owner: None,
            digest: None,
            digest_algorithm: None,
            canonicalization_version: None,
        });
    }

    fn parse_cf_policy_identity_start(&mut self, attrs: &HashMap<Vec<u8>, String>) {
        let policy_id = attr(attrs, b"policy-id").and_then(parse_uuid_urn);
        let pvid = attr(attrs, b"policy-version-id").and_then(parse_uuid_urn);
        if policy_id.is_none() {
            self.warnings.push(Diagnostic::warning(
                "INVALID_CF_IDENTITY",
                "CF policy-identity element is missing a valid 'policy-id' UUID",
            ));
        }
        let state = attr(attrs, b"publication-state").unwrap_or("").to_string();
        if let Some(ref mut rule) = self.current_rule {
            rule.cf_policy_meta = Some(CfPolicyMeta {
                policy_id: policy_id.unwrap_or_else(Uuid::nil),
                policy_version_id: pvid.unwrap_or_else(Uuid::nil),
                publication_state: state,
                enabled_default: attr(attrs, b"enabled-default").and_then(parse_xsd_boolean),
                selected: attr(attrs, b"selected").and_then(parse_xsd_boolean),
                policy_order: attr(attrs, b"policy-order").and_then(|value| value.parse().ok()),
                implementation_state: attr(attrs, b"implementation-state").map(str::to_owned),
                version: None,
                execution_phase: None,
                strict: None,
                policy_type: None,
                config: None,
                compliance_metadata: None,
                dependencies: None,
                digest: None,
                digest_algorithm: None,
                canonicalization_version: None,
            });
        }
    }
}

// ── Classification ────────────────────────────────────────────────────────────

fn parse_uuid_urn(s: &str) -> Option<Uuid> {
    let uuid_str = s.strip_prefix("urn:uuid:").unwrap_or(s);
    Uuid::parse_str(uuid_str).ok()
}

fn classify(state: &mut ParserState) {
    let has_cf_elements = state.saw_supported_cf_content
        || state.cf_bundle_meta.is_some()
        || state.rules.iter().any(|r| r.cf_policy_meta.is_some());

    if state.errors.iter().any(|e| e.blocking) {
        state.class = DocumentClass::InvalidXccdf;
        state.fidelity = Fidelity::Degraded;
        return;
    }

    if state.benchmark.is_none() {
        state.class = DocumentClass::UnsupportedPackage;
        state.fidelity = Fidelity::Degraded;
        state
            .fidelity_losses
            .push("No XCCDF Benchmark element found".into());
        return;
    }

    if has_cf_elements {
        if state.saw_unknown_cf_content {
            // Document uses CF extensions but includes unknown CF elements
            // (e.g. from a newer CF namespace version). It cannot be imported
            // exactly; partial extraction may be possible in a later tool.
            state.class = DocumentClass::CfNativeUnsupportedExtension;
            state.fidelity = Fidelity::Degraded;
            state
                .fidelity_losses
                .push("Document contains unrecognised Crystal Forge extension elements".into());
        } else {
            state.class = DocumentClass::CfNativeExact;
            state.fidelity = Fidelity::NativeExact;
        }
    } else {
        state.class = DocumentClass::ForeignXccdf;
        // Foreign XCCDF: all rules are preserved opaque unless the user maps them.
        state.fidelity = Fidelity::PreservedOpaque;
        for rule in &mut state.rules {
            if rule.checks.is_empty() {
                state.fidelity_losses.push(format!(
                    "Rule '{}' has no check content — will be stored as unbound",
                    rule.id
                ));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal_xccdf() -> &'static str {
        r#"<?xml version="1.0" encoding="UTF-8"?>
<Benchmark xmlns="http://checklists.nist.gov/xccdf/1.2"
    id="xccdf_org.crystalforge_benchmark_test">
  <status>draft</status>
  <title>Test Benchmark</title>
  <version>0.1.0</version>
  <Rule id="xccdf_org.crystalforge_rule_test">
    <title>Test Rule</title>
    <description>Test description</description>
  </Rule>
</Benchmark>"#
    }

    #[test]
    fn parses_valid_xccdf_without_errors() {
        let limits = InterchangeLimits::default();
        let parsed = parse_xccdf(minimal_xccdf().as_bytes(), Some("test.xml"), &limits).unwrap();
        assert_eq!(parsed.class, DocumentClass::ForeignXccdf);
        assert_eq!(parsed.errors.len(), 0);
        assert_eq!(parsed.rules.len(), 1);
        assert_eq!(parsed.rules[0].id, "xccdf_org.crystalforge_rule_test");
    }

    #[test]
    fn parses_check_and_fix_fidelity_fields() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<Benchmark xmlns="http://checklists.nist.gov/xccdf/1.2" id="b">
  <status>draft</status>
  <title>Test</title>
  <version>1</version>
  <Rule id="r">
    <title>Rule</title>
    <fix id="fix-1" system="urn:example:fix" complexity="medium" disruption="low">Apply it</fix>
    <check system="urn:example:check" selector="sel" multi-check="1" negate="0">
      <check-content>Verify it</check-content>
    </check>
  </Rule>
</Benchmark>"#;
        let parsed = parse_xccdf(xml.as_bytes(), None, &InterchangeLimits::default()).unwrap();
        assert!(
            parsed.errors.is_empty(),
            "unexpected errors: {:?}",
            parsed.errors
        );
        let rule = &parsed.rules[0];
        let check = rule.checks.first().expect("check");
        assert_eq!(check.system, "urn:example:check");
        assert_eq!(check.selector.as_deref(), Some("sel"));
        assert_eq!(check.multi_check, Some(true));
        assert_eq!(check.negate, Some(false));
        assert!(matches!(
            check.body_parts.first(),
            Some(CheckBodyPart::Inline { content }) if content == "Verify it"
        ));
        assert_eq!(check.body_parts.len(), 1);
        let fix = rule.fix.as_ref().expect("fix");
        assert_eq!(fix.id.as_deref(), Some("fix-1"));
        assert_eq!(fix.system.as_deref(), Some("urn:example:fix"));
        assert_eq!(fix.complexity.as_deref(), Some("medium"));
        assert_eq!(fix.disruption.as_deref(), Some("low"));
        assert_eq!(fix.content, "Apply it");
    }

    #[test]
    fn parses_check_content_reference() {
        let xml = r#"<Benchmark xmlns="http://checklists.nist.gov/xccdf/1.2" id="b">
  <status>draft</status><title>Test</title><version>1</version>
  <Rule id="r"><title>Rule</title>
    <check system="urn:example:check"><check-content-ref href="checks.xml" name="check-1"/></check>
  </Rule>
</Benchmark>"#;
        let parsed = parse_xccdf(xml.as_bytes(), None, &InterchangeLimits::default()).unwrap();
        let check = parsed.rules[0].checks.first().expect("check");
        assert!(matches!(
            check.body_parts.first(),
            Some(CheckBodyPart::Reference { href, name })
                if href == "checks.xml" && name.as_deref() == Some("check-1")
        ));
        assert_eq!(check.body_parts.len(), 1);
    }

    #[test]
    fn rejects_invalid_check_boolean() {
        let xml = r#"<Benchmark xmlns="http://checklists.nist.gov/xccdf/1.2" id="b">
  <status>draft</status><title>Test</title><version>1</version>
  <Rule id="r"><title>Rule</title>
    <check system="urn:example:check" negate="yes"><check-content>Verify</check-content></check>
  </Rule>
</Benchmark>"#;
        let parsed = parse_xccdf(xml.as_bytes(), None, &InterchangeLimits::default()).unwrap();
        assert!(parsed
            .errors
            .iter()
            .any(|error| error.code == "INVALID_XSD_BOOLEAN"));
    }

    #[test]
    fn rejects_check_without_body() {
        let parsed = parse_xccdf(
            doc_with_body(r#"<Rule id="r"><title>Rule</title><check system="s"/></Rule>"#)
                .as_bytes(),
            None,
            &InterchangeLimits::default(),
        )
        .unwrap();
        assert!(parsed
            .errors
            .iter()
            .any(|error| error.code == "CHECK_BODY_MISSING"));
        assert!(parsed.rules[0].checks.is_empty());
    }

    /// Reference appearing AFTER inline body is invalid per XCCDF sequence.
    #[test]
    fn rejects_check_with_reference_after_inline_body() {
        let parsed = parse_xccdf(
            doc_with_body(
                r#"<Rule id="r"><title>Rule</title><check system="s"><check-content>one</check-content><check-content-ref href="checks.xml"/></check></Rule>"#,
            ).as_bytes(),
            None,
            &InterchangeLimits::default(),
        )
        .unwrap();
        assert!(
            parsed
                .errors
                .iter()
                .any(|error| error.code == "CHECK_REFERENCE_AFTER_INLINE"),
            "reference after inline must produce CHECK_REFERENCE_AFTER_INLINE"
        );
        assert!(parsed.rules[0].checks.is_empty());
    }

    /// Multiple inline bodies are always invalid.
    #[test]
    fn rejects_check_with_two_inline_bodies() {
        let parsed = parse_xccdf(
            doc_with_body(
                r#"<Rule id="r"><title>Rule</title><check system="s"><check-content>one</check-content><check-content>two</check-content></check></Rule>"#,
            ).as_bytes(),
            None,
            &InterchangeLimits::default(),
        )
        .unwrap();
        assert!(
            parsed
                .errors
                .iter()
                .any(|error| error.code == "CHECK_INLINE_DUPLICATE"),
            "two inline elements must produce CHECK_INLINE_DUPLICATE"
        );
        assert!(parsed.rules[0].checks.is_empty());
    }

    /// DISA STIG pattern: reference with inline fallback.
    /// This is the canonical structure used by the Anduril NixOS V1R1 STIG —
    /// 104 rules each contain one check-content-ref followed by check-content.
    #[test]
    fn accepts_check_reference_with_inline_fallback() {
        let parsed = parse_xccdf(
            doc_with_body(
                r#"<Rule id="SV-test-r1_rule"><title>Test Rule</title>
                   <check system="C-test-r1_chk">
                     <check-content-ref href="Anduril_NixOS_STIG.xml" name="M"/>
                     <check-content>Verify the setting is correct.</check-content>
                   </check></Rule>"#,
            )
            .as_bytes(),
            None,
            &InterchangeLimits::default(),
        )
        .unwrap();
        assert!(
            parsed.errors.iter().all(|e| !e.blocking),
            "ref+inline combination must produce no blocking errors, got: {:?}",
            parsed
                .errors
                .iter()
                .filter(|e| e.blocking)
                .collect::<Vec<_>>()
        );
        assert_eq!(parsed.rules.len(), 1);
        let check = parsed.rules[0].checks.first().expect("check");
        assert_eq!(check.body_parts.len(), 2, "must preserve both body parts");
        assert!(
            matches!(&check.body_parts[0], CheckBodyPart::Reference { href, name }
                if href == "Anduril_NixOS_STIG.xml" && name.as_deref() == Some("M")),
            "first body part must be the reference"
        );
        assert!(
            matches!(&check.body_parts[1], CheckBodyPart::Inline { content } if !content.is_empty()),
            "second body part must be the inline fallback"
        );
    }

    /// Multiple references followed by one inline fallback — also valid.
    #[test]
    fn preserves_multiple_references_followed_by_inline_fallback() {
        let parsed = parse_xccdf(
            doc_with_body(
                r#"<Rule id="r"><title>Rule</title>
                   <check system="s">
                     <check-content-ref href="a.xml" name="A"/>
                     <check-content-ref href="b.xml"/>
                     <check-content>Manual fallback text.</check-content>
                   </check></Rule>"#,
            )
            .as_bytes(),
            None,
            &InterchangeLimits::default(),
        )
        .unwrap();
        assert!(
            parsed.errors.iter().all(|e| !e.blocking),
            "multiple refs+inline must produce no blocking errors"
        );
        let check = parsed.rules[0].checks.first().expect("check");
        assert_eq!(
            check.body_parts.len(),
            3,
            "must preserve all three body parts"
        );
        assert!(
            matches!(&check.body_parts[0], CheckBodyPart::Reference { href, .. } if href == "a.xml")
        );
        assert!(
            matches!(&check.body_parts[1], CheckBodyPart::Reference { href, .. } if href == "b.xml")
        );
        assert!(matches!(&check.body_parts[2], CheckBodyPart::Inline { .. }));
    }

    #[test]
    fn accepts_check_with_multiple_reference_bodies() {
        let parsed = parse_xccdf(
            doc_with_body(
                r#"<Rule id="r"><title>Rule</title><check system="s"><check-content-ref href="one.xml"/><check-content-ref href="two.xml"/></check></Rule>"#,
            ).as_bytes(),
            None,
            &InterchangeLimits::default(),
        )
        .unwrap();
        // Multiple check-content-ref elements are valid in XCCDF 1.2.
        assert!(
            !parsed
                .errors
                .iter()
                .any(|error| error.code == "CHECK_BODY_AMBIGUOUS"),
            "multiple references must not produce CHECK_BODY_AMBIGUOUS"
        );
        let check = parsed.rules[0].checks.first().expect("check");
        assert_eq!(check.body_parts.len(), 2);
        assert!(
            matches!(&check.body_parts[0], CheckBodyPart::Reference { href, .. } if href == "one.xml")
        );
        assert!(
            matches!(&check.body_parts[1], CheckBodyPart::Reference { href, .. } if href == "two.xml")
        );
    }

    #[test]
    fn rejects_oversized_upload() {
        let limits = InterchangeLimits {
            max_xml_bytes: 10,
            ..InterchangeLimits::default()
        };
        let big = vec![b'x'; 100];
        let result = parse_xccdf(&big, None, &limits);
        assert!(result.is_err());
    }

    #[test]
    fn computes_source_sha256() {
        let limits = InterchangeLimits::default();
        let parsed = parse_xccdf(minimal_xccdf().as_bytes(), None, &limits).unwrap();
        let expected = hex::encode(Sha256::digest(minimal_xccdf().as_bytes()));
        assert_eq!(parsed.source_sha256, expected);
    }

    #[test]
    fn handles_empty_document() {
        let limits = InterchangeLimits::default();
        let parsed = parse_xccdf(b"", None, &limits).unwrap();
        assert_eq!(parsed.class, DocumentClass::UnsupportedPackage);
    }

    #[test]
    fn rejects_dtd() {
        let limits = InterchangeLimits::default();
        let dtd = r#"<?xml version="1.0"?>
<!DOCTYPE Benchmark [<!ENTITY x "y">]>
<Benchmark xmlns="http://checklists.nist.gov/xccdf/1.2" id="xccdf_test_benchmark">
  <status>draft</status>
  <title>Test</title>
  <version>0.1</version>
</Benchmark>"#;
        let parsed = parse_xccdf(dtd.as_bytes(), None, &limits).unwrap();
        assert!(parsed.errors.iter().any(|e| e.code == "DTD_FORBIDDEN"));
    }

    #[test]
    fn rejects_malformed_entity() {
        let limits = InterchangeLimits::default();
        let xml = r#"<?xml version="1.0"?>
<Benchmark xmlns="http://checklists.nist.gov/xccdf/1.2" id="xccdf_test_benchmark">
  <status>draft</status>
  <title>&invalid;</title>
  <version>0.1</version>
</Benchmark>"#;
        let parsed = parse_xccdf(xml.as_bytes(), None, &limits).unwrap();
        assert!(parsed.errors.iter().any(|e| e.code == "XML_ENTITY_ERROR"));
    }

    #[test]
    fn enforces_rule_limit_and_stops() {
        let limits = InterchangeLimits {
            max_rule_count: 2,
            ..InterchangeLimits::default()
        };
        let mut xml = String::from(
            r#"<?xml version="1.0"?>
<Benchmark xmlns="http://checklists.nist.gov/xccdf/1.2" id="xccdf_test_benchmark">
  <status>draft</status>
  <title>Test</title>
  <version>0.1</version>"#,
        );
        for i in 0..10 {
            xml.push_str(&format!("<Rule id=\"r{}\"><title>R{}</title></Rule>", i, i));
        }
        xml.push_str("</Benchmark>");
        let parsed = parse_xccdf(xml.as_bytes(), None, &limits).unwrap();
        assert!(parsed
            .errors
            .iter()
            .any(|e| e.code == "RULE_LIMIT_EXCEEDED"));
        assert!(parsed.rules.len() <= 2);
    }

    #[test]
    fn enforces_attribute_limit() {
        let limits = InterchangeLimits {
            max_attributes_per_element: 2,
            ..InterchangeLimits::default()
        };
        let xml = r#"<?xml version="1.0"?>
<Benchmark xmlns="http://checklists.nist.gov/xccdf/1.2" id="a" style="b" resolved="true" extra="c">
  <status>draft</status>
  <title>Test</title>
  <version>0.1</version>
</Benchmark>"#;
        let parsed = parse_xccdf(xml.as_bytes(), None, &limits).unwrap();
        assert!(parsed
            .errors
            .iter()
            .any(|e| e.code == "ATTRIBUTE_LIMIT_EXCEEDED"));
    }

    #[test]
    fn enforces_cumulative_text_limit() {
        let limits = InterchangeLimits {
            max_text_node_bytes: 10,
            ..InterchangeLimits::default()
        };
        let xml = r#"<?xml version="1.0"?>
<Benchmark xmlns="http://checklists.nist.gov/xccdf/1.2" id="xccdf_test">
  <status>draft</status>
  <title>This is a long title that exceeds ten bytes</title>
  <version>0.1</version>
</Benchmark>"#;
        let parsed = parse_xccdf(xml.as_bytes(), None, &limits).unwrap();
        assert!(parsed.errors.iter().any(|e| e.code == "TEXT_TOO_LARGE"));
    }

    // ── Namespace resolution tests ────────────────────────────────────────────

    /// Standard `cf` prefix bound to the CF extension namespace is accepted.
    #[test]
    fn standard_cf_prefix_classifies_as_native() {
        let limits = InterchangeLimits::default();
        let xml = format!(
            r#"<?xml version="1.0"?>
<Benchmark xmlns="{XCCDF_NAMESPACE}" xmlns:cf="{CF_XCCDF_NAMESPACE}"
    id="xccdf_cf_benchmark_test">
  <status>draft</status>
  <title>Test</title>
  <version>0.1</version>
  <cf:bundle bundle-id="urn:uuid:11111111-1111-1111-1111-111111111111"
      bundle-version-id="urn:uuid:22222222-2222-2222-2222-222222222222"
      publication-state="published"/>
</Benchmark>"#
        );
        let parsed = parse_xccdf(xml.as_bytes(), None, &limits).unwrap();
        assert_eq!(parsed.class, DocumentClass::CfNativeExact);
        assert_eq!(parsed.errors.len(), 0);
        assert!(parsed.cf_bundle_meta.is_some());
    }

    /// A different prefix bound to the same CF namespace URI must also be
    /// accepted: resolution is by URI, not by literal prefix.
    #[test]
    fn alternate_prefix_with_cf_namespace_classifies_as_native() {
        let limits = InterchangeLimits::default();
        let xml = format!(
            r#"<?xml version="1.0"?>
<Benchmark xmlns="{XCCDF_NAMESPACE}" xmlns:crystal="{CF_XCCDF_NAMESPACE}"
    id="xccdf_cf_benchmark_test">
  <status>draft</status>
  <title>Test</title>
  <version>0.1</version>
  <crystal:bundle bundle-id="urn:uuid:11111111-1111-1111-1111-111111111111"
      bundle-version-id="urn:uuid:22222222-2222-2222-2222-222222222222"
      publication-state="published"/>
</Benchmark>"#
        );
        let parsed = parse_xccdf(xml.as_bytes(), None, &limits).unwrap();
        assert_eq!(parsed.class, DocumentClass::CfNativeExact);
        assert!(parsed.cf_bundle_meta.is_some());
    }

    /// The literal `cf` prefix bound to a different URI must not produce
    /// CF-native classification.
    #[test]
    fn misleading_cf_prefix_is_not_classified_as_native() {
        let limits = InterchangeLimits::default();
        let xml = format!(
            r#"<?xml version="1.0"?>
<Benchmark xmlns="{XCCDF_NAMESPACE}" xmlns:cf="urn:not-crystal-forge"
    id="xccdf_cf_benchmark_test">
  <status>draft</status>
  <title>Test</title>
  <version>0.1</version>
  <cf:bundle id="bundle-1"/>
</Benchmark>"#
        );
        let parsed = parse_xccdf(xml.as_bytes(), None, &limits).unwrap();
        assert_ne!(parsed.class, DocumentClass::CfNativeExact);
        assert!(parsed.cf_bundle_meta.is_none());
    }

    /// An undeclared prefix is a blocking error.
    #[test]
    fn undeclared_prefix_is_a_blocking_error() {
        let limits = InterchangeLimits::default();
        let xml = format!(
            r#"<?xml version="1.0"?>
<Benchmark xmlns="{XCCDF_NAMESPACE}" id="xccdf_cf_benchmark_test">
  <status>draft</status>
  <title>Test</title>
  <version>0.1</version>
  <cf:bundle id="bundle-1"/>
</Benchmark>"#
        );
        let parsed = parse_xccdf(xml.as_bytes(), None, &limits).unwrap();
        assert!(parsed
            .errors
            .iter()
            .any(|e| e.code == "XML_UNKNOWN_NAMESPACE_PREFIX" && e.blocking));
        assert_eq!(parsed.class, DocumentClass::InvalidXccdf);
    }

    /// An unqualified child inside a CF element is not CF content: it stays in
    /// the inherited default (XCCDF) namespace and must not populate CF
    /// metadata.
    #[test]
    fn unqualified_child_is_not_treated_as_cf() {
        let limits = InterchangeLimits::default();
        let xml = format!(
            r#"<?xml version="1.0"?>
<Benchmark xmlns="{XCCDF_NAMESPACE}" xmlns:cf="{CF_XCCDF_NAMESPACE}"
    id="xccdf_cf_benchmark_test">
  <status>draft</status>
  <title>Test</title>
  <version>0.1</version>
  <cf:bundle bundle-id="urn:uuid:11111111-1111-1111-1111-111111111111"
      bundle-version-id="urn:uuid:22222222-2222-2222-2222-222222222222"
      publication-state="published">
    <framework>not-cf-content</framework>
  </cf:bundle>
</Benchmark>"#
        );
        let parsed = parse_xccdf(xml.as_bytes(), None, &limits).unwrap();
        let meta = parsed.cf_bundle_meta.expect("bundle metadata parsed");
        assert!(meta.framework.is_none());
    }

    /// The CF namespace applied as a subtree default namespace (no prefix at
    /// all) must resolve by URI.
    #[test]
    fn default_cf_namespace_resolves_by_uri() {
        let limits = InterchangeLimits::default();
        let xml = format!(
            r#"<?xml version="1.0"?>
<Benchmark xmlns="{XCCDF_NAMESPACE}" id="xccdf_cf_benchmark_test">
  <status>draft</status>
  <title>Test</title>
  <version>0.1</version>
  <bundle xmlns="{CF_XCCDF_NAMESPACE}"
      bundle-id="urn:uuid:11111111-1111-1111-1111-111111111111"
      bundle-version-id="urn:uuid:22222222-2222-2222-2222-222222222222"
      publication-state="published"/>
</Benchmark>"#
        );
        let parsed = parse_xccdf(xml.as_bytes(), None, &limits).unwrap();
        assert_eq!(parsed.class, DocumentClass::CfNativeExact);
        assert!(parsed.cf_bundle_meta.is_some());
    }

    // ── Duplicate identity detection ──────────────────────────────────────────

    fn doc_with_body(body: &str) -> String {
        format!(
            r#"<?xml version="1.0"?>
<Benchmark xmlns="{XCCDF_NAMESPACE}" id="xccdf_test_benchmark">
  <status>draft</status>
  <title>Test</title>
  <version>0.1</version>
{body}
</Benchmark>"#
        )
    }

    #[test]
    fn duplicate_ids_are_rejected_for_every_category() {
        let limits = InterchangeLimits::default();
        let cases: &[(&str, &str)] = &[
            (r#"<Rule id="r1"/><Rule id="r1"/>"#, "DUPLICATE_RULE_ID"),
            (
                r#"<Profile id="p1"/><Profile id="p1"/>"#,
                "DUPLICATE_PROFILE_ID",
            ),
            (r#"<Group id="g1"/><Group id="g1"/>"#, "DUPLICATE_GROUP_ID"),
            (r#"<Value id="v1"/><Value id="v1"/>"#, "DUPLICATE_VALUE_ID"),
        ];
        for (body, code) in cases {
            let parsed = parse_xccdf(doc_with_body(body).as_bytes(), None, &limits).unwrap();
            assert!(
                parsed.errors.iter().any(|e| e.code == *code && e.blocking),
                "expected blocking {code} for body {body}"
            );
            assert_eq!(parsed.class, DocumentClass::InvalidXccdf);
        }
    }

    #[test]
    fn duplicate_benchmark_id_is_rejected() {
        let limits = InterchangeLimits::default();
        let xml = format!(
            r#"<?xml version="1.0"?>
<Benchmark xmlns="{XCCDF_NAMESPACE}" id="xccdf_dup">
  <Benchmark id="xccdf_dup"/>
</Benchmark>"#
        );
        let parsed = parse_xccdf(xml.as_bytes(), None, &limits).unwrap();
        assert!(parsed
            .errors
            .iter()
            .any(|e| e.code == "DUPLICATE_BENCHMARK_ID" && e.blocking));
    }

    #[test]
    fn duplicate_rule_id_aborts_collection() {
        let limits = InterchangeLimits::default();
        let body = r#"<Rule id="r1"/><Rule id="r1"/><Rule id="r2"/>"#;
        let parsed = parse_xccdf(doc_with_body(body).as_bytes(), None, &limits).unwrap();
        assert!(parsed.errors.iter().any(|e| e.code == "DUPLICATE_RULE_ID"));
        // Parsing aborted: only the first rule was collected.
        assert_eq!(parsed.rules.len(), 1);
    }

    // ── Group and value limits ────────────────────────────────────────────────

    #[test]
    fn enforces_group_limit_and_stops() {
        let limits = InterchangeLimits {
            max_group_count: 1,
            ..InterchangeLimits::default()
        };
        let body = r#"<Group id="g1"/><Group id="g2"/><Group id="g3"/>"#;
        let parsed = parse_xccdf(doc_with_body(body).as_bytes(), None, &limits).unwrap();
        assert!(parsed
            .errors
            .iter()
            .any(|e| e.code == "GROUP_LIMIT_EXCEEDED" && e.blocking));
        assert!(parsed.groups.len() <= 1);
    }

    #[test]
    fn enforces_value_limit_and_stops() {
        let limits = InterchangeLimits {
            max_value_count: 1,
            ..InterchangeLimits::default()
        };
        let body = r#"<Value id="v1"/><Value id="v2"/><Value id="v3"/>"#;
        let parsed = parse_xccdf(doc_with_body(body).as_bytes(), None, &limits).unwrap();
        assert!(parsed
            .errors
            .iter()
            .any(|e| e.code == "VALUE_LIMIT_EXCEEDED" && e.blocking));
        assert!(parsed.values.len() <= 1);
    }

    // ── Attribute validation tests ────────────────────────────────────────────

    #[test]
    fn rejects_malformed_attribute() {
        let limits = InterchangeLimits::default();
        // Inject a raw broken attribute (unclosed quote). quick-xml surfaces
        // this as an attribute parse error.
        let xml = b"<?xml version=\"1.0\"?>\
<Benchmark xmlns=\"http://checklists.nist.gov/xccdf/1.2\" id=bad>\
</Benchmark>";
        let parsed = parse_xccdf(xml, None, &limits).unwrap();
        assert!(
            parsed
                .errors
                .iter()
                .any(|e| e.code == "XML_ATTRIBUTE_ERROR" && e.blocking),
            "expected XML_ATTRIBUTE_ERROR, got: {:?}",
            parsed.errors
        );
    }

    #[test]
    fn rejects_duplicate_attribute() {
        let limits = InterchangeLimits::default();
        // XML spec disallows duplicate attribute names; quick-xml reports them
        // as attribute errors in strict mode.
        let xml = format!(
            r#"<?xml version="1.0"?>
<Benchmark xmlns="{XCCDF_NAMESPACE}" id="b1" id="b2"></Benchmark>"#
        );
        let parsed = parse_xccdf(xml.as_bytes(), None, &limits).unwrap();
        assert!(
            parsed.errors.iter().any(|e| {
                (e.code == "XML_ATTRIBUTE_ERROR" || e.code == "DUPLICATE_ATTRIBUTE") && e.blocking
            }),
            "expected attribute error for duplicate 'id', got: {:?}",
            parsed.errors
        );
    }

    #[test]
    fn attribute_count_exactly_at_limit_is_accepted() {
        let limits = InterchangeLimits {
            max_attributes_per_element: 3,
            ..InterchangeLimits::default()
        };
        // 3 attributes: xmlns, id, style — exactly at limit.
        let xml = format!(
            r#"<?xml version="1.0"?>
<Benchmark xmlns="{XCCDF_NAMESPACE}" id="b1" style="base">
  <status>draft</status>
  <title>Test</title>
  <version>0.1</version>
</Benchmark>"#
        );
        let parsed = parse_xccdf(xml.as_bytes(), None, &limits).unwrap();
        assert!(
            !parsed
                .errors
                .iter()
                .any(|e| e.code == "ATTRIBUTE_LIMIT_EXCEEDED"),
            "3 attrs at limit-3 should be accepted"
        );
    }

    #[test]
    fn attribute_count_one_above_limit_is_rejected() {
        let limits = InterchangeLimits {
            max_attributes_per_element: 3,
            ..InterchangeLimits::default()
        };
        // 4 attributes: xmlns, id, style, resolved — one over limit.
        let xml = format!(
            r#"<?xml version="1.0"?>
<Benchmark xmlns="{XCCDF_NAMESPACE}" id="b1" style="base" resolved="1">
  <status>draft</status>
  <title>Test</title>
  <version>0.1</version>
</Benchmark>"#
        );
        let parsed = parse_xccdf(xml.as_bytes(), None, &limits).unwrap();
        assert!(
            parsed
                .errors
                .iter()
                .any(|e| e.code == "ATTRIBUTE_LIMIT_EXCEEDED" && e.blocking),
            "4 attrs above limit-3 should be rejected"
        );
    }

    // ── CF classification tests ───────────────────────────────────────────────

    #[test]
    fn recognised_cf_only_classifies_as_exact() {
        let limits = InterchangeLimits::default();
        let xml = format!(
            r#"<?xml version="1.0"?>
<Benchmark xmlns="{XCCDF_NAMESPACE}" xmlns:cf="{CF_XCCDF_NAMESPACE}"
    id="xccdf_cf_benchmark_test">
  <status>draft</status>
  <title>Test</title>
  <version>0.1</version>
  <cf:bundle bundle-id="urn:uuid:11111111-1111-1111-1111-111111111111"
      bundle-version-id="urn:uuid:22222222-2222-2222-2222-222222222222"
      publication-state="published"/>
</Benchmark>"#
        );
        let parsed = parse_xccdf(xml.as_bytes(), None, &limits).unwrap();
        assert_eq!(parsed.class, DocumentClass::CfNativeExact);
        assert_eq!(parsed.fidelity, Fidelity::NativeExact);
    }

    #[test]
    fn unknown_cf_element_downgrades_to_unsupported_extension() {
        let limits = InterchangeLimits::default();
        // A recognised bundle plus an unknown CF element should downgrade.
        let xml = format!(
            r#"<?xml version="1.0"?>
<Benchmark xmlns="{XCCDF_NAMESPACE}" xmlns:cf="{CF_XCCDF_NAMESPACE}"
    id="xccdf_cf_benchmark_test">
  <status>draft</status>
  <title>Test</title>
  <version>0.1</version>
  <cf:bundle bundle-id="urn:uuid:11111111-1111-1111-1111-111111111111"
      bundle-version-id="urn:uuid:22222222-2222-2222-2222-222222222222"
      publication-state="published"/>
  <cf:future-element/>
</Benchmark>"#
        );
        let parsed = parse_xccdf(xml.as_bytes(), None, &limits).unwrap();
        assert_eq!(parsed.class, DocumentClass::CfNativeUnsupportedExtension);
        assert_eq!(parsed.fidelity, Fidelity::Degraded);
    }

    #[test]
    fn newer_cf_namespace_version_classifies_as_foreign() {
        let limits = InterchangeLimits::default();
        // A different CF namespace URI is treated as an Other namespace, not CF.
        let xml = format!(
            r#"<?xml version="1.0"?>
<Benchmark xmlns="{XCCDF_NAMESPACE}" xmlns:cf="urn:crystal-forge:xccdf:2"
    id="xccdf_cf_benchmark_test">
  <status>draft</status>
  <title>Test</title>
  <version>0.1</version>
  <cf:bundle id="bundle-1"/>
</Benchmark>"#
        );
        let parsed = parse_xccdf(xml.as_bytes(), None, &limits).unwrap();
        // "cf" prefix bound to a future CF namespace URI → Other → no CF metadata.
        assert_ne!(parsed.class, DocumentClass::CfNativeExact);
        assert_ne!(parsed.class, DocumentClass::CfNativeUnsupportedExtension);
        assert!(parsed.cf_bundle_meta.is_none());
    }

    #[test]
    fn invalid_cf_bundle_uuid_produces_warning() {
        let limits = InterchangeLimits::default();
        let xml = format!(
            r#"<?xml version="1.0"?>
<Benchmark xmlns="{XCCDF_NAMESPACE}" xmlns:cf="{CF_XCCDF_NAMESPACE}"
    id="xccdf_cf_benchmark_test">
  <status>draft</status>
  <title>Test</title>
  <version>0.1</version>
  <cf:bundle bundle-id="not-a-uuid"
      bundle-version-id="urn:uuid:22222222-2222-2222-2222-222222222222"
      publication-state="published"/>
</Benchmark>"#
        );
        let parsed = parse_xccdf(xml.as_bytes(), None, &limits).unwrap();
        assert!(
            parsed
                .warnings
                .iter()
                .any(|w| w.code == "INVALID_CF_IDENTITY"),
            "expected INVALID_CF_IDENTITY warning for bad UUID"
        );
        // Despite the bad UUID the classification still reflects CF content.
        assert_eq!(parsed.class, DocumentClass::CfNativeExact);
    }

    // ── XCCDF 1.1 namespace support ───────────────────────────────────────────

    /// Minimal XCCDF 1.1.4 document that structurally mirrors the real
    /// Anduril NixOS V1R2 STIG (downloaded from public.cyber.mil).
    fn nixos_stig_fixture() -> String {
        // The real STIG uses xmlns="http://checklists.nist.gov/xccdf/1.1"
        // as the default namespace with no prefix, plus several auxiliary
        // namespaces (Dublin Core, CPE, XHTML, XML DSig).  This minimal
        // fixture reproduces that structure.
        format!(
            r#"<?xml version="1.0" encoding="utf-8"?>
<Benchmark xmlns:dc="http://purl.org/dc/elements/1.1/"
    xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"
    xmlns:cpe="http://cpe.mitre.org/language/2.0"
    xmlns:xhtml="http://www.w3.org/1999/xhtml"
    xsi:schemaLocation="{XCCDF_1_1_NAMESPACE} http://nvd.nist.gov/schema/xccdf-1.1.4.xsd"
    id="Anduril_NixOS_STIG_fixture"
    xml:lang="en"
    xmlns="{XCCDF_1_1_NAMESPACE}">
  <status date="2025-08-19">accepted</status>
  <title>Anduril NixOS Security Technical Implementation Guide (Fixture)</title>
  <description>Minimal fixture derived from U_Anduril_NixOS_V1R2_STIG.</description>
  <reference href="https://cyber.mil">
    <dc:publisher>DISA</dc:publisher>
    <dc:source>STIG.DOD.MIL</dc:source>
  </reference>
  <version>1</version>
  <Profile id="MAC-1_Classified">
    <title>I - Mission Critical Classified</title>
    <select idref="V-268078" selected="true"/>
  </Profile>
  <Group id="V-268078">
    <title>GEN000000</title>
    <Rule id="SV-268078r1_rule" severity="medium">
      <title>NixOS must be configured to use an approved cryptographic module.</title>
      <description>Without approved cryptographic algorithms, information cannot be protected.</description>
      <check system="C-268078r1_chk">
        <check-content>Verify the NixOS cryptographic configuration.</check-content>
      </check>
      <fix id="F-268078r1_fix">Apply the approved cryptographic configuration.</fix>
    </Rule>
  </Group>
</Benchmark>"#
        )
    }

    #[test]
    fn parses_xccdf_1_1_document_as_foreign_xccdf() {
        let limits = InterchangeLimits::default();
        let xml = nixos_stig_fixture();
        let parsed = parse_xccdf(xml.as_bytes(), Some("fixture.xml"), &limits).unwrap();

        // No blocking errors; 1.1 is a valid import source.
        assert!(
            !parsed.errors.iter().any(|e| e.blocking),
            "unexpected blocking errors: {:?}",
            parsed.errors
        );
        // Classified as foreign XCCDF, not invalid.
        assert_eq!(parsed.class, DocumentClass::ForeignXccdf);
        // Namespace version is detected and reported.
        assert_eq!(parsed.xccdf_namespace_version, Some("1.1"));
        // Structural content is preserved.
        assert!(parsed.benchmark.is_some());
        assert_eq!(parsed.rules.len(), 1);
        assert_eq!(parsed.rules[0].id, "SV-268078r1_rule");
        assert_eq!(parsed.profiles.len(), 1);
    }

    #[test]
    fn detects_xccdf_namespace_version_as_1_2_for_standard_document() {
        let limits = InterchangeLimits::default();
        let xml = format!(
            r#"<?xml version="1.0"?>
<Benchmark xmlns="{XCCDF_NAMESPACE}" id="b1">
  <status>draft</status>
  <title>T</title>
  <version>0.1</version>
</Benchmark>"#
        );
        let parsed = parse_xccdf(xml.as_bytes(), None, &limits).unwrap();
        assert_eq!(parsed.xccdf_namespace_version, Some("1.2"));
    }

    /// Real artifact regression: U_Anduril_NixOS_V1R1_STIG.zip
    /// Every one of the 104 rules contains a check with one check-content-ref
    /// followed by one check-content (manual fallback).  This combination was
    /// incorrectly rejected as CHECK_BODY_AMBIGUOUS before this fix.
    ///
    /// Run with the real artifact:
    ///   CF_TEST_ANDURIL_STIG_ZIP=.../U_Anduril_NixOS_V1R1_STIG.zip \
    ///   cargo test anduril_nixos_v1r1_stig -- --ignored --nocapture
    #[test]
    #[ignore = "reads real artifact from CF_TEST_ANDURIL_STIG_ZIP env var"]
    fn anduril_nixos_v1r1_stig_parses_without_blocking_errors() {
        let path = std::env::var("CF_TEST_ANDURIL_STIG_ZIP")
            .expect("CF_TEST_ANDURIL_STIG_ZIP must be set");
        let zip_bytes = std::fs::read(&path).expect("read zip");
        let pkg = crate::compliance::xccdf::package::process_xccdf_bytes(
            zip_bytes,
            Some(
                std::path::Path::new(&path)
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned(),
            ),
            &InterchangeLimits::default(),
        )
        .expect("process_xccdf_bytes must succeed");

        let parsed = &pkg.parsed;
        let blocking: Vec<_> = parsed.errors.iter().filter(|e| e.blocking).collect();
        assert!(
            blocking.is_empty(),
            "must have no blocking errors, got: {:?}",
            blocking
        );

        let ambiguous: Vec<_> = parsed
            .errors
            .iter()
            .filter(|e| e.code.contains("AMBIGUOUS"))
            .collect();
        assert!(
            ambiguous.is_empty(),
            "must have 0 CHECK_BODY_AMBIGUOUS errors, got {}",
            ambiguous.len()
        );

        assert!(
            parsed.rules.len() >= 100,
            "expected at least 100 rules, got {}",
            parsed.rules.len()
        );

        let ref_and_inline: usize = parsed
            .rules
            .iter()
            .flat_map(|r| r.checks.iter())
            .filter(|check| {
                let has_ref = check
                    .body_parts
                    .iter()
                    .any(|p| matches!(p, CheckBodyPart::Reference { .. }));
                let has_inline = check
                    .body_parts
                    .iter()
                    .any(|p| matches!(p, CheckBodyPart::Inline { .. }));
                has_ref && has_inline
            })
            .count();
        println!("Rules: {}", parsed.rules.len());
        println!("Profiles: {}", parsed.profiles.len());
        println!("Checks with ref+inline: {}", ref_and_inline);
        assert!(
            ref_and_inline > 0,
            "expected checks with both ref and inline fallback"
        );
    }

    /// Deterministic fixture that mirrors the Anduril STIG V1R1 check pattern.
    /// Unlike the ignored real-artifact test, this always runs.
    #[test]
    fn anduril_stig_check_pattern_fixture() {
        // 3 rules × check with 1 ref + 1 inline (the actual STIG pattern).
        let body = (1..=3)
            .map(|i| {
                format!(
                    r#"<Rule id="SV-{i:06}r1_rule" severity="medium">
                     <title>Rule {i}</title>
                     <check system="C-{i:06}r1_chk">
                       <check-content-ref href="Anduril_NixOS_STIG.xml" name="M"/>
                       <check-content>Verify step {i}.</check-content>
                     </check>
                   </Rule>"#
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        let xml = format!(
            r#"<?xml version="1.0" encoding="utf-8"?>
<Benchmark xmlns="http://checklists.nist.gov/xccdf/1.1"
    id="Anduril_NixOS_STIG" xml:lang="en">
  <status date="2024-10-25">accepted</status>
  <title>Anduril NixOS STIG Fixture</title>
  <version>1</version>
  {body}
</Benchmark>"#
        );
        let limits = InterchangeLimits::default();
        let parsed = parse_xccdf(xml.as_bytes(), Some("fixture.xml"), &limits).unwrap();

        let blocking: Vec<_> = parsed.errors.iter().filter(|e| e.blocking).collect();
        assert!(
            blocking.is_empty(),
            "STIG check pattern must produce no blocking errors, got: {:?}",
            blocking
        );
        assert_eq!(parsed.rules.len(), 3, "3 rules expected");
        for rule in &parsed.rules {
            let check = rule.checks.first().expect("each rule has a check");
            assert_eq!(check.body_parts.len(), 2, "ref+inline = 2 body parts");
            assert!(
                matches!(&check.body_parts[0], CheckBodyPart::Reference { href, name }
                    if href == "Anduril_NixOS_STIG.xml" && name.as_deref() == Some("M"))
            );
            assert!(matches!(&check.body_parts[1], CheckBodyPart::Inline { .. }));
        }
    }

    #[test]
    fn foreign_extension_namespace_classifies_as_foreign_xccdf() {
        let limits = InterchangeLimits::default();
        let xml = format!(
            r#"<?xml version="1.0"?>
<Benchmark xmlns="{XCCDF_NAMESPACE}" xmlns:ext="urn:some-other-vendor:ext:1"
    id="xccdf_cf_benchmark_test">
  <status>draft</status>
  <title>Test</title>
  <version>0.1</version>
  <Rule id="r1">
    <title>Rule 1</title>
    <ext:vendor-data/>
  </Rule>
</Benchmark>"#
        );
        let parsed = parse_xccdf(xml.as_bytes(), None, &limits).unwrap();
        assert_eq!(parsed.class, DocumentClass::ForeignXccdf);
        assert!(parsed.cf_bundle_meta.is_none());
    }
}
