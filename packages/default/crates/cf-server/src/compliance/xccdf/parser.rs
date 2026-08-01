//! Secure XCCDF 1.2 and CF-XCCDF extension parser.
//!
//! Uses `quick-xml` with DTD and entity processing disabled. Accepts raw bytes
//! from file upload or ZIP extraction. Classifies documents and returns typed
//! structures for preview and import.

use anyhow::Result;
use quick_xml::Reader;
use quick_xml::events::Event;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use super::super::interchange::{
    CF_NIX_FIX_SYSTEM, CF_POLICY_CHECK_SYSTEM, CF_XCCDF_NAMESPACE, InterchangeLimits,
    XCCDF_NAMESPACE,
};
use super::models::*;

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
    let mut reader = Reader::from_reader(std::io::Cursor::new(bytes));
    reader.config_mut().trim_text(true);

    let mut state = ParserState::new(limits);
    state.filename = filename.map(String::from);
    state.source_bytes = bytes.to_vec();
    state.source_sha256 = sha256.clone();

    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                let name = String::from_utf8_lossy(e.name().local_name().into_inner().as_ref())
                    .to_string();
                let name: &str = name.as_str();
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
                state.handle_start(name, e);
            }
            Ok(Event::End(ref e)) => {
                let name = String::from_utf8_lossy(e.name().local_name().into_inner().as_ref())
                    .to_string();
                let name: &str = name.as_str();
                state.depth = state.depth.saturating_sub(1);
                state.handle_end(name);
            }
            Ok(Event::Text(ref e)) => {
                match e.unescape() {
                    Ok(text) => state.handle_text(&text),
                    Err(error) => {
                        state.errors.push(Diagnostic::error(
                            "XML_ENTITY_ERROR",
                            &format!("Invalid XML entity reference: {error}"),
                        ));
                        break;
                    }
                }
            }
            Ok(Event::CData(ref e)) => {
                let bytes: &[u8] = &**e;
                if let Ok(text) = std::str::from_utf8(bytes) {
                    state.handle_text(text);
                }
            }
            Ok(Event::Eof) => break,
            Ok(Event::DocType(_)) => {
                state.errors.push(Diagnostic::error(
                    "DTD_FORBIDDEN",
                    "DOCTYPE declarations and DTDs are not accepted",
                ));
                break;
            }
            Ok(Event::Empty(ref e)) => {
                let name = std::str::from_utf8(e.name().local_name().into_inner()).unwrap_or("");
                state.handle_start(name, e);
                state.handle_end(name);
            }
            Ok(Event::Decl(_)) => {}
            Ok(Event::Comment(_)) => {}
            Err(e) => {
                state.errors.push(Diagnostic {
                    code: "XML_PARSE_ERROR".into(),
                    summary: format!("XML parse error: {e}"),
                    field: None,
                    xml_line: None,
                    xml_column: None,
                    object_identity: None,
                    blocking: true,
                    remediation: Some("Verify the XML document is well-formed.".into()),
                });
                break;
            }
            _ => {}
        }
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

    // Parsing state stack.
    in_xccdf: bool,
    in_cf_ns: bool,
    current_element: String,
    current_text: String,
    current_rule: Option<ParsedRule>,
    current_profile: Option<ParsedProfile>,
    current_group: Option<ParsedGroup>,
    current_check: Option<CheckContent>,
    current_fix: Option<FixContent>,
    current_check_system: Option<String>,
    current_ident: Option<StandardIdentifier>,
    current_ref: Option<Reference>,
    in_check_content: bool,
    in_fix_content: bool,
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
            in_xccdf: false,
            in_cf_ns: false,
            current_element: String::new(),
            current_text: String::new(),
            current_rule: None,
            current_profile: None,
            current_group: None,
            current_check: None,
            current_fix: None,
            current_check_system: None,
            current_ident: None,
            current_ref: None,
            in_check_content: false,
            in_fix_content: false,
        }
    }

    fn handle_start(&mut self, name: &str, e: &quick_xml::events::BytesStart) {
        self.current_element = name.to_string();
        self.current_text.clear();

        // Enforce attribute count limit.
        let attr_count = e.attributes().count();
        if attr_count > self.limits.max_attributes_per_element {
            self.errors.push(Diagnostic::error(
                "ATTRIBUTE_LIMIT_EXCEEDED",
                &format!(
                    "Element '{}' has {} attributes, exceeding maximum {}",
                    name, attr_count, self.limits.max_attributes_per_element
                ),
            ));
            return;
        }

        // Detect namespace prefix on this element.
        let is_cf = e.name().prefix()
            .and_then(|p| std::str::from_utf8(p.as_ref()).ok().map(String::from))
            .as_deref()
            .map(|p| p == "cf")
            .unwrap_or(false);

        // Detect namespace via xmlns attribute on Benchmark.
        if name == "Benchmark" && !is_cf {
            self.in_xccdf = true;
            if let Some(ns) = e.try_get_attribute("xmlns").ok().flatten() {
                if let Ok(val) = std::str::from_utf8(&ns.value) {
                    if val != XCCDF_NAMESPACE {
                        self.warnings.push(Diagnostic::warning(
                            "UNKNOWN_NAMESPACE",
                            &format!(
                                "Expected XCCDF namespace '{}', got '{}'",
                                XCCDF_NAMESPACE, val
                            ),
                        ));
                    }
                }
            }
            self.xccdf_version = e
                .try_get_attribute("xccdf:version")
                .ok()
                .flatten()
                .and_then(|v| std::str::from_utf8(&v.value).ok().map(String::from));
        }

        // Detect CF extension namespace via prefix.
        if is_cf {
            self.in_cf_ns = true;
        }

        match name {
            "Benchmark" => self.parse_benchmark_start(e),
            "title" => {}
            "description" => {}
            "version" => {
                self.current_text.clear(); /* will be captured in text handler */
            }
            "status" => {
                if let Some(date) = e.try_get_attribute("date").ok().flatten() {
                    if let Ok(d) = std::str::from_utf8(&date.value) {
                        if let Some(ref mut bm) = self.benchmark {
                            bm.status_date = Some(d.to_string());
                        }
                    }
                }
            }
            "platform" => {
                if let Some(idref) = e.try_get_attribute("idref").ok().flatten() {
                    if let Ok(v) = std::str::from_utf8(&idref.value) {
                        if let Some(ref mut bm) = self.benchmark {
                            bm.platforms.push(v.to_string());
                        }
                    }
                }
            }
            "reference" => {
                let href = e
                    .try_get_attribute("href")
                    .ok()
                    .flatten()
                    .and_then(|v| std::str::from_utf8(&v.value).ok().map(String::from));
                self.current_ref = Some(Reference { href, title: None });
            }
            "Profile" => self.parse_profile_start(e),
            "Rule" => self.parse_rule_start(e),
            "Group" => self.parse_group_start(e),
            "Value" => self.parse_value_start(e),
            "check" => self.parse_check_start(e),
            "check-content" => {
                self.in_check_content = true;
                self.current_text.clear();
            }
            "fix" => self.parse_fix_start(e),
            "fixtext" => {
                self.in_fix_content = true;
                self.current_text.clear();
            }
            "ident" => {
                let system = e
                    .try_get_attribute("system")
                    .ok()
                    .flatten()
                    .and_then(|v| std::str::from_utf8(&v.value).ok().map(String::from))
                    .unwrap_or_default();
                self.current_ident = Some(StandardIdentifier {
                    system,
                    value: String::new(),
                });
            }
            // CF extension elements (detected by prefix, matched by local name).
            "bundle" if is_cf => self.parse_cf_bundle_start(e),
            "policy-identity" if is_cf => self.parse_cf_policy_identity_start(e),
            "policy" if is_cf => { /* policy body; child elements handled below */ }
            "framework" if is_cf => {}
            "content-digest" if is_cf => {}
            "execution" if is_cf => {
                if let Some(ref mut meta) = self
                    .current_rule
                    .as_mut()
                    .and_then(|r| r.cf_policy_meta.as_mut())
                {
                    if let Some(phase) = e.try_get_attribute("phase").ok().flatten() {
                        meta.execution_phase =
                            std::str::from_utf8(&phase.value).ok().map(String::from);
                    }
                    if let Some(strict) = e.try_get_attribute("strict").ok().flatten() {
                        meta.strict = std::str::from_utf8(&strict.value).ok().map(|s| s == "true");
                    }
                }
            }
            _ => {
                // Preserve unknown content in current rule for fidelity.
                if self.current_rule.is_some() {
                    self.current_text.clear();
                    self.current_text.push_str("<");
                    self.current_text.push_str(name);
                    self.current_text.push('>');
                }
            }
        }
    }

    fn handle_end(&mut self, name: &str) {
        match name {
            "Benchmark" => {
                self.in_xccdf = false;
            }
            "title" => {
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
            "description" => {
                if let Some(ref mut bm) = self.benchmark {
                    bm.description = bm.description.clone().or(Some(self.current_text.clone()));
                }
                if let Some(ref mut rule) = self.current_rule {
                    rule.description = Some(self.current_text.clone());
                }
            }
            "version" => {
                if let Some(ref mut bm) = self.benchmark {
                    bm.version = Some(self.current_text.clone());
                }
            }
            "status" => {
                if let Some(ref mut bm) = self.benchmark {
                    bm.status = Some(self.current_text.clone());
                }
            }
            "reference" => {
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
            "Profile" => {
                if let Some(pr) = self.current_profile.take() {
                    self.profiles.push(pr);
                }
            }
            "Rule" => {
                if let Some(rule) = self.current_rule.take() {
                    self.rules.push(rule);
                }
            }
            "Group" => {
                if let Some(grp) = self.current_group.take() {
                    self.groups.push(grp);
                }
            }
            "Value" => { /* values already pushed */ }
            "check" => {
                let check = self.current_check.take();
                if let Some(ref mut rule) = self.current_rule {
                    rule.check = check;
                }
            }
            "check-content" => {
                self.in_check_content = false;
                if let Some(ref mut check) = self.current_check {
                    check.content = self.current_text.clone();
                }
            }
            "fix" => {
                let fix = self.current_fix.take();
                if let Some(ref mut rule) = self.current_rule {
                    rule.fix = fix;
                }
            }
            "fixtext" => {
                self.in_fix_content = false;
            }
            "ident" => {
                let ident = self.current_ident.take();
                if let Some(ref mut rule) = self.current_rule {
                    if let Some(i) = ident {
                        rule.identifiers.push(i);
                    }
                }
            }
            "select" => {
                if let Some(ref mut pr) = self.current_profile {
                    pr.select_ids.push(self.current_text.clone());
                }
            }
            "rationale" => {
                if let Some(ref mut rule) = self.current_rule {
                    rule.rationale = Some(self.current_text.clone());
                }
            }
            "bundle" if self.in_cf_ns => { /* bundle meta already parsed */ }
            "framework" if self.in_cf_ns => {
                if let Some(ref mut meta) = self.cf_bundle_meta {
                    meta.framework = Some(self.current_text.clone());
                }
            }
            "layer" if self.in_cf_ns => {
                if let Some(ref mut meta) = self.cf_bundle_meta {
                    meta.layer = Some(self.current_text.clone());
                }
            }
            "owner" if self.in_cf_ns => {
                if let Some(ref mut meta) = self.cf_bundle_meta {
                    meta.owner = Some(self.current_text.clone());
                }
            }
            "content-digest" if self.in_cf_ns => {
                if let Some(ref mut meta) = self
                    .current_rule
                    .as_mut()
                    .and_then(|r| r.cf_policy_meta.as_mut())
                {
                    meta.digest = Some(self.current_text.clone());
                }
                if let Some(ref mut meta) = self.cf_bundle_meta {
                    meta.digest = Some(self.current_text.clone());
                }
            }
            "policy-version" if self.in_cf_ns => {
                if let Some(ref mut meta) = self
                    .current_rule
                    .as_mut()
                    .and_then(|r| r.cf_policy_meta.as_mut())
                {
                    meta.version = Some(self.current_text.clone());
                }
            }
            _ => {}
        }

        // Reset CF namespace tracking when leaving an element.
        if self.in_cf_ns {
            self.in_cf_ns = false;
        }

        self.current_element.clear();
    }

    fn handle_text(&mut self, text: &str) {
        if self.current_text.len() + text.len() > self.limits.max_text_node_bytes {
            self.errors.push(Diagnostic::error(
                "TEXT_TOO_LARGE",
                &format!(
                    "Cumulative text {} exceeds maximum of {} bytes",
                    self.current_text.len() + text.len(),
                    self.limits.max_text_node_bytes
                ),
            ));
            return;
        }
        self.current_text.push_str(text);

        // Pass ident text.
        if let Some(ref mut ident) = self.current_ident {
            ident.value = self.current_text.clone();
        }
    }

    fn begin_rule(&mut self) -> bool {
        if self.rules.len() >= self.limits.max_rule_count {
            self.errors.push(Diagnostic::error(
                "RULE_LIMIT_EXCEEDED",
                &format!(
                    "Rule count exceeds maximum {}",
                    self.limits.max_rule_count
                ),
            ));
            return false;
        }
        true
    }

    fn begin_profile(&mut self) -> bool {
        if self.profiles.len() >= self.limits.max_profile_count {
            self.errors.push(Diagnostic::error(
                "PROFILE_LIMIT_EXCEEDED",
                &format!(
                    "Profile count exceeds maximum {}",
                    self.limits.max_profile_count
                ),
            ));
            return false;
        }
        true
    }

    // ── Start-element parsing ──────────────────────────────────────────────────

    fn parse_benchmark_start(&mut self, e: &quick_xml::events::BytesStart) {
        let id = e
            .try_get_attribute("id")
            .ok()
            .flatten()
            .and_then(|v| std::str::from_utf8(&v.value).ok().map(String::from))
            .unwrap_or_default();
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
    }

    fn parse_profile_start(&mut self, e: &quick_xml::events::BytesStart) {
        if !self.begin_profile() {
            return;
        }
        let id = e
            .try_get_attribute("id")
            .ok()
            .flatten()
            .and_then(|v| std::str::from_utf8(&v.value).ok().map(String::from))
            .unwrap_or_default();
        let extends = e
            .try_get_attribute("extends")
            .ok()
            .flatten()
            .and_then(|v| std::str::from_utf8(&v.value).ok().map(String::from));
        let abstract_attr = e
            .try_get_attribute("abstract")
            .ok()
            .flatten()
            .and_then(|v| std::str::from_utf8(&v.value).ok().map(|s| s == "true"))
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
    }

    fn parse_rule_start(&mut self, e: &quick_xml::events::BytesStart) {
        if !self.begin_rule() {
            return;
        }
        let id = e
            .try_get_attribute("id")
            .ok()
            .flatten()
            .and_then(|v| std::str::from_utf8(&v.value).ok().map(String::from))
            .unwrap_or_default();
        let severity = e
            .try_get_attribute("severity")
            .ok()
            .flatten()
            .and_then(|v| std::str::from_utf8(&v.value).ok().map(String::from));
        let weight = e.try_get_attribute("weight").ok().flatten().and_then(|v| {
            std::str::from_utf8(&v.value)
                .ok()
                .and_then(|s| s.parse::<f64>().ok())
        });
        self.current_rule = Some(ParsedRule {
            id,
            title: None,
            description: None,
            rationale: None,
            severity,
            weight,
            version: None,
            check: None,
            fix: None,
            identifiers: vec![],
            references: vec![],
            platforms: vec![],
            group_id: self.current_group.as_ref().map(|g| g.id.clone()),
            rule_order: Some(self.rules.len()),
            cf_policy_meta: None,
            preserved_xml: None,
        });
    }

    fn parse_group_start(&mut self, e: &quick_xml::events::BytesStart) {
        let id = e
            .try_get_attribute("id")
            .ok()
            .flatten()
            .and_then(|v| std::str::from_utf8(&v.value).ok().map(String::from))
            .unwrap_or_default();
        self.current_group = Some(ParsedGroup {
            id,
            title: None,
            description: None,
            rule_ids: vec![],
        });
    }

    fn parse_value_start(&mut self, e: &quick_xml::events::BytesStart) {
        let id = e
            .try_get_attribute("id")
            .ok()
            .flatten()
            .and_then(|v| std::str::from_utf8(&v.value).ok().map(String::from))
            .unwrap_or_default();
        let vtype = e
            .try_get_attribute("type")
            .ok()
            .flatten()
            .and_then(|v| std::str::from_utf8(&v.value).ok().map(String::from))
            .unwrap_or("string".to_string());
        self.values.push(ParsedValue {
            id,
            title: None,
            description: None,
            value_type: vtype.to_string(),
            default_value: None,
            allowed_values: vec![],
        });
    }

    fn parse_check_start(&mut self, e: &quick_xml::events::BytesStart) {
        let system = e
            .try_get_attribute("system")
            .ok()
            .flatten()
            .and_then(|v| std::str::from_utf8(&v.value).ok().map(String::from))
            .unwrap_or_default();
        let selector = e
            .try_get_attribute("selector")
            .ok()
            .flatten()
            .and_then(|v| std::str::from_utf8(&v.value).ok().map(String::from));
        self.current_check_system = Some(system.to_string());
        self.current_check = Some(CheckContent {
            system: system.to_string(),
            content: String::new(),
            selector,
        });
    }

    fn parse_fix_start(&mut self, e: &quick_xml::events::BytesStart) {
        let system = e
            .try_get_attribute("system")
            .ok()
            .flatten()
            .and_then(|v| std::str::from_utf8(&v.value).ok().map(String::from));
        let complexity = e
            .try_get_attribute("complexity")
            .ok()
            .flatten()
            .and_then(|v| std::str::from_utf8(&v.value).ok().map(String::from));
        let disruption = e
            .try_get_attribute("disruption")
            .ok()
            .flatten()
            .and_then(|v| std::str::from_utf8(&v.value).ok().map(String::from));
        self.current_fix = Some(FixContent {
            system,
            content: String::new(),
            complexity,
            disruption,
        });
    }

    fn parse_cf_bundle_start(&mut self, e: &quick_xml::events::BytesStart) {
        let bundle_id = e
            .try_get_attribute("bundle-id")
            .ok()
            .flatten()
            .and_then(|v| std::str::from_utf8(&v.value).ok().map(parse_uuid_urn))
            .flatten();
        let bvid = e
            .try_get_attribute("bundle-version-id")
            .ok()
            .flatten()
            .and_then(|v| std::str::from_utf8(&v.value).ok().map(parse_uuid_urn))
            .flatten();
        let state = e
            .try_get_attribute("publication-state")
            .ok()
            .flatten()
            .and_then(|v| std::str::from_utf8(&v.value).ok().map(String::from))
            .unwrap_or_default();
        self.cf_bundle_meta = Some(CfBundleMeta {
            bundle_id: bundle_id.unwrap_or_else(Uuid::nil),
            bundle_version_id: bvid.unwrap_or_else(Uuid::nil),
            publication_state: state,
            framework: None,
            framework_version: None,
            layer: None,
            owner: None,
            digest: None,
        });
    }

    fn parse_cf_policy_identity_start(&mut self, e: &quick_xml::events::BytesStart) {
        let policy_id = e
            .try_get_attribute("policy-id")
            .ok()
            .flatten()
            .and_then(|v| std::str::from_utf8(&v.value).ok().map(parse_uuid_urn))
            .flatten();
        let pvid = e
            .try_get_attribute("policy-version-id")
            .ok()
            .flatten()
            .and_then(|v| std::str::from_utf8(&v.value).ok().map(parse_uuid_urn))
            .flatten();
        let state = e
            .try_get_attribute("publication-state")
            .ok()
            .flatten()
            .and_then(|v| std::str::from_utf8(&v.value).ok().map(String::from))
            .unwrap_or_default();
        if let Some(ref mut rule) = self.current_rule {
            rule.cf_policy_meta = Some(CfPolicyMeta {
                policy_id: policy_id.unwrap_or_else(Uuid::nil),
                policy_version_id: pvid.unwrap_or_else(Uuid::nil),
                publication_state: state,
                version: None,
                execution_phase: None,
                strict: None,
                policy_type: None,
                config: None,
                digest: None,
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
    let has_cf_elements =
        state.cf_bundle_meta.is_some() || state.rules.iter().any(|r| r.cf_policy_meta.is_some());

    if state.errors.iter().any(|e| e.blocking) {
        state.class = DocumentClass::InvalidXccdf;
        state.fidelity = Fidelity::Degraded;
        return;
    }

    if state.benchmark.is_none() {
        state.class = DocumentClass::UnsupportedPackage;
        state.fidelity = Fidelity::Degraded;
        if state.benchmark.is_none() {
            state
                .fidelity_losses
                .push("No XCCDF Benchmark element found".into());
        }
        return;
    }

    if has_cf_elements {
        if state
            .rules
            .iter()
            .any(|r| r.cf_policy_meta.is_none() && !r.id.is_empty())
        {
            state.class = DocumentClass::CfNativeExact;
            state.fidelity = Fidelity::NativeExact;
        } else {
            state.class = DocumentClass::CfNativeExact;
            state.fidelity = Fidelity::NativeExact;
        }
    } else {
        state.class = DocumentClass::ForeignXccdf;
        // Foreign XCCDF: all rules are preserved opaque unless the user maps them.
        state.fidelity = Fidelity::PreservedOpaque;
        for rule in &mut state.rules {
            if rule.check.is_some() {
                // A standard check exists; the user may map it later.
            } else {
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
}
