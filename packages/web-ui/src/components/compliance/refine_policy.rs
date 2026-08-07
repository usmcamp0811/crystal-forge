use dioxus::prelude::*;

use crate::api::models::{
    ImportedCustomCheck, ImportedCustomCheckRule, ImportedEvidenceRequirement,
    ImportedPolicyCustomization, XccdfRuleImportAction,
};
use crate::components::icon::{Icon, IconName};

#[derive(Clone, PartialEq, Debug)]
pub struct SourceCheck {
    pub system: String,
    pub selector: Option<String>,
    pub references: Vec<String>,
    pub inline_content: Option<String>,
}

#[derive(Clone, PartialEq, Debug)]
pub struct SourceStigRule {
    pub rule_id: String,
    pub group_id: Option<String>,
    pub stig_id: Option<String>,
    pub title: Option<String>,
    pub description: Option<String>,
    pub source_severity: Option<String>,
    pub fix_text: Option<String>,
    pub checks: Vec<SourceCheck>,
    pub identifiers: Vec<String>,
    pub references: Vec<String>,
    pub platforms: Vec<String>,
    pub rule_order: usize,
}

#[derive(Clone, PartialEq, Debug)]
pub enum RefinedRuleAction { Native, Manual, Unbound, Opaque, Existing(Option<uuid::Uuid>) }

#[derive(Clone, PartialEq, Debug)]
pub enum ComparisonOperator { Equal, NotEqual, GreaterOrEqual, LessOrEqual }
impl ComparisonOperator { fn as_str(&self) -> &'static str { match self { Self::Equal => "==", Self::NotEqual => "!=", Self::GreaterOrEqual => ">=", Self::LessOrEqual => "<=" } } }

#[derive(Clone, PartialEq, Debug)]
pub enum TypedPolicyValue { Boolean(bool), Integer(String), String(String), Null, List(String), AttributeSet(String) }
impl TypedPolicyValue { fn as_nix(&self) -> String { match self { Self::Boolean(v) => v.to_string(), Self::Integer(v) | Self::List(v) | Self::AttributeSet(v) => v.clone(), Self::String(v) => format!("\"{}\"", v.replace('"', "\\\"")), Self::Null => "null".into() } } }

#[derive(Clone, PartialEq, Debug)]
pub enum PolicyAssertionDraft {
    NixosOption { path: String, operator: ComparisonOperator, expected_value: TypedPolicyValue, failure_message: String, strict: bool },
    PackagesInstalled { packages: Vec<String>, failure_message: String, strict: bool },
    CustomExpression { field_name: String, expression: String, failure_message: String, strict: bool },
}

impl PolicyAssertionDraft {
    fn to_rule(&self, index: usize) -> ImportedCustomCheckRule {
        match self {
            Self::NixosOption { path, operator, expected_value, failure_message, strict } => ImportedCustomCheckRule { field_name: format!("nixosOption{index}"), expression: format!("cfg.config.{path} {} {}", operator.as_str(), expected_value.as_nix()), description: failure_message.clone(), strict: *strict },
            Self::PackagesInstalled { packages, failure_message, strict } => {
                let required = packages.iter().map(|package| format!("\"{}\"", package.replace('"', "\\\""))).collect::<Vec<_>>().join(" ");
                ImportedCustomCheckRule { field_name: format!("packagesInstalled{index}"), expression: format!("builtins.all (required: builtins.any (package: (package.pname or (package.name or \"\")) == required) cfg.config.environment.systemPackages) [ {required} ]"), description: failure_message.clone(), strict: *strict }
            }
            Self::CustomExpression { field_name, expression, failure_message, strict } => ImportedCustomCheckRule { field_name: field_name.clone(), expression: expression.clone(), description: failure_message.clone(), strict: *strict },
        }
    }
}

#[derive(Clone, PartialEq, Debug)]
pub enum EvidenceRequirementDraft { Command { command: String, expected_output: String }, File { path: String, expected_content: String }, UnitState { unit: String, state: String }, Log { source: String, unit: Option<String>, pattern: String }, Attestation { description: String } }
impl EvidenceRequirementDraft {
    fn to_requirement(&self) -> ImportedEvidenceRequirement { match self { Self::Command { command, expected_output } => ImportedEvidenceRequirement::Command { command: command.clone(), expected_output: expected_output.clone() }, Self::File { path, expected_content } => ImportedEvidenceRequirement::File { path: path.clone(), expected_content: expected_content.clone() }, Self::UnitState { unit, state } => ImportedEvidenceRequirement::UnitState { unit: unit.clone(), state: state.clone() }, Self::Log { source, unit, pattern } => ImportedEvidenceRequirement::Log { source: source.clone(), unit: unit.clone(), pattern: pattern.clone() }, Self::Attestation { description } => ImportedEvidenceRequirement::Attestation { description: description.clone() } } }
}

#[derive(Clone, PartialEq, Debug)]
pub struct RefinedPolicyDraft {
    pub local_name: String,
    pub local_description: String,
    pub local_severity: String,
    pub local_rationale: String,
    pub implementation_note: String,
    pub action: RefinedRuleAction,
    pub assertion_mode: String,
    pub assertions: Vec<PolicyAssertionDraft>,
    pub evidence_requirements: Vec<EvidenceRequirementDraft>,
}

#[derive(Clone, PartialEq, Debug)]
pub struct RefinedStigRule { pub source: SourceStigRule, pub draft: RefinedPolicyDraft, pub selected: bool }
impl RefinedStigRule {
    pub fn is_valid(&self) -> bool { match &self.draft.action { RefinedRuleAction::Native => !self.draft.assertions.is_empty(), RefinedRuleAction::Existing(id) => id.is_some(), _ => true } }
}

pub fn action_to_import(rule: &RefinedStigRule) -> XccdfRuleImportAction {
    let c = ImportedPolicyCustomization { policy_name: Some(rule.draft.local_name.clone()), policy_description: Some(rule.draft.local_description.clone()), implementation_note: (!rule.draft.implementation_note.trim().is_empty()).then(|| rule.draft.implementation_note.clone()), policy_severity: Some(rule.draft.local_severity.clone()), policy_rationale: (!rule.draft.local_rationale.trim().is_empty()).then(|| rule.draft.local_rationale.clone()) };
    match &rule.draft.action {
        RefinedRuleAction::Native => XccdfRuleImportAction::CreateNativeCustom { rule_id: rule.source.rule_id.clone(), customization: c, custom_check: ImportedCustomCheck { mode: rule.draft.assertion_mode.clone(), rules: rule.draft.assertions.iter().enumerate().map(|(i, a)| a.to_rule(i + 1)).collect() }, evidence_requirements: rule.draft.evidence_requirements.iter().map(EvidenceRequirementDraft::to_requirement).collect() },
        RefinedRuleAction::Manual => XccdfRuleImportAction::CreateManual { rule_id: rule.source.rule_id.clone(), customization: c, evidence_requirements: rule.draft.evidence_requirements.iter().map(EvidenceRequirementDraft::to_requirement).collect() },
        RefinedRuleAction::Unbound => XccdfRuleImportAction::CreateUnbound { rule_id: rule.source.rule_id.clone(), customization: c },
        RefinedRuleAction::Opaque => XccdfRuleImportAction::PreserveOpaque { rule_id: rule.source.rule_id.clone(), customization: c },
        RefinedRuleAction::Existing(Some(id)) => XccdfRuleImportAction::MapExisting { rule_id: rule.source.rule_id.clone(), policy_version_id: *id },
        RefinedRuleAction::Existing(None) => XccdfRuleImportAction::CreateUnbound { rule_id: rule.source.rule_id.clone(), customization: c },
    }
}

#[derive(Props, Clone, PartialEq)]
pub struct RefinePolicyStepProps { pub rules: Signal<Vec<RefinedStigRule>>, pub cursor: Signal<usize>, pub existing_policies: Vec<(uuid::Uuid, String)>, pub on_back: EventHandler<()>, pub on_review: EventHandler<()> }

#[component]
pub fn RefinePolicyStep(mut props: RefinePolicyStepProps) -> Element {
    let selected: Vec<usize> = props.rules.read().iter().enumerate().filter_map(|(i, r)| r.selected.then_some(i)).collect();
    let position = (*props.cursor.read()).min(selected.len().saturating_sub(1));
    let Some(index) = selected.get(position).copied() else { return rsx! { div {} }; };
    let rule = props.rules.read()[index].clone();
    let source_id = source_identity(&rule.source);
    let source_srg = rule.source.identifiers.iter().find(|identifier| identifier.starts_with("SRG-")).cloned();
    let percent = ((position + 1) as f32 / selected.len().max(1) as f32 * 100.0).round();
    rsx! {
        div { class: "modal-head refine-modal-head",
            div { class: "refine-header", h2 { style: "display:flex;gap:8px;align-items:center;min-width:0;white-space:nowrap;", Icon { name: IconName::Shield, size: 15 }, "Refine policy {position + 1} of {selected.len()}" }, span { class: "mono refine-source-id", title: "{source_id}", "{source_id}" } }
            div { class: "refine-progress", "data-testid": "xccdf-refine-progress", div { class: "refine-progress__value", style: "width:{percent}%;" } }
        }
        div { class: "modal-body refine-modal-body",
            div { class: "refine-identity-row", span { class: "chip chip-info", "Security & hardening" }, if let Some(srg) = source_srg.as_ref() { span { class: "mono refine-srg", "{srg}" } }, span { class: "refine-identity-spacer" }, span { class: "refine-policy-id", "policy id: " span { class: "mono", "stig-{slugify(&source_id)}" } } }
            SourceStigCard { rule: rule.source.clone() }
            div { class: "refine-basics", div { class: "field", label { "Policy name" }, input { class: "input focus-ring mono", "data-testid": "xccdf-policy-name", value: "{rule.draft.local_name}", oninput: move |e| { props.rules.write()[index].draft.local_name = e.value(); } } }, div { class: "field", label { "Severity" }, div { class: "seg refine-severity", for (value, label) in [("high","CAT I"),("medium","CAT II"),("low","CAT III")] { button { class: if rule.draft.local_severity == value { format!("active severity-{value}") } else { String::new() }, onclick: move |_| { props.rules.write()[index].draft.local_severity = value.into(); }, "{label}" } } } } }
            if !matches!(rule.draft.action, RefinedRuleAction::Existing(_) | RefinedRuleAction::Opaque) { AssertionSection { rules: props.rules, index } EvidenceSection { rules: props.rules, index } }
            div { class: "sd-callout sd-callout-info refine-summary", Icon { name: IconName::Check, size: 13 } div { "On import this becomes a standard CF security policy — " strong { "{rule.draft.assertions.len()} config assertions" } " and " strong { "{rule.draft.evidence_requirements.len()} evidence items" } " for ATO. Editable later from the Policies view." } }
            details { class: "refine-advanced", summary { "Advanced import options" }, div { class: "refine-advanced-body", ImplementationChoice { rules: props.rules, index, existing_policies: props.existing_policies.clone() } if matches!(rule.draft.action, RefinedRuleAction::Unbound) { div { class: "sd-callout sd-callout-info", "This requirement will be imported without a Crystal Forge implementation unless you add an assertion or evidence source." } } if matches!(rule.draft.action, RefinedRuleAction::Opaque) { div { class: "sd-callout sd-callout-info", "The original XCCDF rule and check content will be preserved without execution." } } textarea { class: "input focus-ring", rows: 2, placeholder: "Description", value: "{rule.draft.local_description}", oninput: move |e| { props.rules.write()[index].draft.local_description = e.value(); } } textarea { class: "input focus-ring", rows: 2, placeholder: "Rationale", value: "{rule.draft.local_rationale}", oninput: move |e| { props.rules.write()[index].draft.local_rationale = e.value(); } } textarea { class: "input focus-ring", rows: 2, placeholder: "Implementation note", value: "{rule.draft.implementation_note}", oninput: move |e| { props.rules.write()[index].draft.implementation_note = e.value(); } } } }
        }
        div { class: "modal-foot refine-modal-foot", div { class: "refine-footer-actions", button { class: "btn btn-ghost focus-ring", "data-testid": "xccdf-refine-back", onclick: move |_| if position == 0 { props.on_back.call(()) } else { props.cursor.set(position - 1) }, Icon { name: IconName::ArrowLeft, size: 13 }, if position == 0 { " Back to list" } else { " Previous" } }, button { class: "btn btn-ghost focus-ring refine-exclude", title: "Exclude this control from the bundle", "data-testid": "xccdf-refine-exclude", onclick: move |_| { let mut remaining = selected.len().saturating_sub(1); props.rules.write()[index].selected = false; if remaining == 0 { props.on_back.call(()); } else { if position >= remaining { remaining = remaining.saturating_sub(1); } props.cursor.set(position.min(remaining)); } }, "Exclude" } }, div { class: "refine-footer-progress", span { class: "refine-position", "data-testid": "xccdf-refine-position", "{position + 1} / {selected.len()}" }, if position + 1 < selected.len() { button { class: "btn btn-primary focus-ring", "data-testid": "xccdf-refine-next", onclick: move |_| props.cursor.set(position + 1), "Next ", Icon { name: IconName::ChevronRight, size: 13 } } } else { button { class: "btn btn-primary focus-ring", disabled: !props.rules.read().iter().filter(|r| r.selected).all(RefinedStigRule::is_valid), onclick: move |_| props.on_review.call(()), "Review import ", Icon { name: IconName::ChevronRight, size: 13 } } } } }
    }
}

fn source_identity(source: &SourceStigRule) -> String {
    source.stig_id.clone().filter(|id| id.starts_with("V-"))
        .or_else(|| source.identifiers.iter().find(|id| id.starts_with("V-")).cloned())
        .or_else(|| source.group_id.clone().filter(|id| id.starts_with("V-")))
        .unwrap_or_else(|| source.rule_id.clone())
}

#[derive(Props, Clone, PartialEq)]
pub struct ImportReviewProps {
    pub rules: Signal<Vec<RefinedStigRule>>,
    pub on_back: EventHandler<()>,
    pub on_confirm: EventHandler<()>,
}

#[component]
pub fn ImportReview(props: ImportReviewProps) -> Element {
    let selected = props.rules.read().iter().filter(|rule| rule.selected).cloned().collect::<Vec<_>>();
    let native = selected.iter().filter(|rule| matches!(rule.draft.action, RefinedRuleAction::Native)).count();
    let manual = selected.iter().filter(|rule| matches!(rule.draft.action, RefinedRuleAction::Manual)).count();
    let unresolved = selected.iter().filter(|rule| matches!(rule.draft.action, RefinedRuleAction::Unbound | RefinedRuleAction::Opaque)).count();
    rsx! {
        div { class: "modal-head", h2 { "Review policy choices" }, p { class: "page-subtitle", "Confirm the selected policy mappings before creating the draft bundle." } }
        div { class: "modal-body",
            div { class: "stat-strip", div { class: "stat", div { class: "stat-label", "Selected" } div { class: "stat-value", "{selected.len()}" } } div { class: "stat", div { class: "stat-label", "Native" } div { class: "stat-value", "{native}" } } div { class: "stat", div { class: "stat-label", "Manual" } div { class: "stat-value", "{manual}" } } div { class: "stat", div { class: "stat-label", "Unresolved" } div { class: "stat-value", "{unresolved}" } } }
            if unresolved > 0 { div { class: "sd-callout sd-callout-warn", "Unbound and opaque controls will remain visible but will not be executable." } }
            div { style: "display:grid;gap:6px;margin-top:12px;", for rule in selected.iter() { div { class: "card", style: "padding:9px 11px;display:flex;gap:10px;align-items:center;", span { class: "mono", style: "font-size:10px;", "{rule.source.stig_id.clone().unwrap_or_else(|| rule.source.rule_id.clone())}" } div { style: "flex:1;min-width:0;", strong { "{rule.draft.local_name}" } div { class: "text-xs text-gray-500", "{action_label(&rule.draft.action)}" } } } } }
        div { class: "modal-foot", style: "justify-content:space-between;", button { class: "btn btn-ghost", onclick: move |_| props.on_back.call(()), "Back to refine" } button { class: "btn btn-primary", disabled: selected.is_empty(), onclick: move |_| props.on_confirm.call(()), "Create draft bundle" } }
        }
    }
}

fn action_label(action: &RefinedRuleAction) -> &'static str {
    match action { RefinedRuleAction::Native => "Native assertion", RefinedRuleAction::Manual => "Manual evidence", RefinedRuleAction::Unbound => "Unbound", RefinedRuleAction::Opaque => "Opaque", RefinedRuleAction::Existing(_) => "Mapped existing policy" }
}

#[component]
fn SourceStigCard(rule: SourceStigRule) -> Element {
    let severity = rule.source_severity.as_deref().unwrap_or("medium");
    let cat = match severity { "high" => "CAT I", "low" => "CAT III", _ => "CAT II" };
    let stig_id = source_identity(&rule);
    let title = rule.title.clone().unwrap_or_else(|| rule.rule_id.clone());
    let fix_text = rule.fix_text.clone();
    let check_text = rule.checks.iter().filter_map(|check| check.inline_content.clone().or_else(|| check.selector.clone()).or_else(|| (!check.system.is_empty()).then(|| check.system.clone()))).collect::<Vec<_>>().join("\n\n");
    let references = rule.references.join(", ");
    rsx! {
        div { class: "refine-source-card", "data-testid": "xccdf-source-details",
            div { class: "refine-source-card__head",
                Icon { name: IconName::Shield, size: 12 }
                span { class: "refine-source-label", "From the STIG" }
                span { class: "mono refine-source-card__id", "{stig_id}" }
                span { class: "refine-identity-spacer" }
                span { class: "refine-cat severity-{severity}", "{cat}" }
            }
            div { class: "refine-source-card__body",
                div { class: "refine-source-title", "{title}" }
                if let Some(fix) = fix_text { div { div { class: "refine-source-section-label", "Official fix" } div { class: "refine-source-copy", "{fix}" } } }
                if !check_text.is_empty() { div { div { class: "refine-source-section-label", "Official check" } div { class: "mono refine-source-copy", "{check_text}" } } }
                if let Some(description) = rule.description { details { class: "refine-source-more", summary { "Additional source metadata" } p { "{description}" } if !references.is_empty() { p { class: "mono", "References: {references}" } } } }
            }
        }
    }
}

#[component]
fn ImplementationChoice(rules: Signal<Vec<RefinedStigRule>>, index: usize, existing_policies: Vec<(uuid::Uuid, String)>) -> Element {
    let current = action_key(&rules.read()[index].draft.action);
    rsx! { div { style: "margin:14px 0;", label { style: "font-size:11px;font-weight:650;", "Implementation" }, select { class: "input focus-ring", "data-testid": "xccdf-implementation-selector", value: current, onchange: move |event| { set_action(&mut rules.write()[index].draft.action, event.value().as_str()); }, option { value: "unbound", "Unbound" }, option { value: "native", "Native assertion" }, option { value: "manual", "Manual evidence" }, option { value: "opaque", "Opaque" }, option { value: "existing", "Existing policy version" } }
        if current == "existing" {
            select { class: "input focus-ring", value: "{existing_policy_value(&rules.read()[index].draft.action)}", onchange: move |event| { let selected = uuid::Uuid::parse_str(&event.value()).ok(); rules.write()[index].draft.action = RefinedRuleAction::Existing(selected); }, option { value: "", "Select an existing policy version…" }, for (id, name) in existing_policies.iter() { option { value: "{id}", "{name}" } } }
        }
    } }
}
fn existing_policy_value(action: &RefinedRuleAction) -> String { match action { RefinedRuleAction::Existing(Some(id)) => id.to_string(), _ => String::new() } }
fn action_key(action: &RefinedRuleAction) -> &'static str { match action { RefinedRuleAction::Native => "native", RefinedRuleAction::Manual => "manual", RefinedRuleAction::Unbound => "unbound", RefinedRuleAction::Opaque => "opaque", RefinedRuleAction::Existing(_) => "existing" } }
fn set_action(action: &mut RefinedRuleAction, key: &str) { *action = match key { "native" => RefinedRuleAction::Native, "manual" => RefinedRuleAction::Manual, "opaque" => RefinedRuleAction::Opaque, "existing" => RefinedRuleAction::Existing(None), _ => RefinedRuleAction::Unbound }; }

#[component]
fn AssertionSection(rules: Signal<Vec<RefinedStigRule>>, index: usize) -> Element {
    let count = rules.read()[index].draft.assertions.len();
    rsx! {
        section { class: "refine-section",
            div { class: "refine-section-label", "NixOS config assertions" span { class: "refine-eval-badge", "EVAL-TIME" } }
            p { class: "refine-helper", "Asserted against the rendered config during " span { class: "mono", "nix flake check" } " — fails the build before it deploys. These become the policy's rules. " if count == 0 { "Nothing was inferred for this control — add the assertion that proves it." } else { "Inferred from the STIG; review before importing." } }
            if count == 0 { div { class: "refine-assertion-empty", Icon { name: IconName::Warn, size: 13 } div { "No assertion could be inferred from this STIG control. Add one — assert a NixOS option value, assert a package is installed, or write a custom nix expression — or leave empty to rely on runtime evidence alone." } } }
            for (assertion_index, assertion) in rules.read()[index].draft.assertions.iter().cloned().enumerate() {
                AssertionEditor { rules, index, assertion_index, assertion }
            }
            select {
                class: "input focus-ring refine-add-assertion",
                value: "",
                onchange: move |e| {
                    let draft = match e.value().as_str() {
                        "option" => PolicyAssertionDraft::NixosOption { path: String::new(), operator: ComparisonOperator::Equal, expected_value: TypedPolicyValue::String(String::new()), failure_message: "Option assertion failed".into(), strict: true },
                        "packages" => PolicyAssertionDraft::PackagesInstalled { packages: vec![], failure_message: "Required package is not installed".into(), strict: true },
                        _ => PolicyAssertionDraft::CustomExpression { field_name: format!("customAssertion{}", count + 1), expression: String::new(), failure_message: "Custom assertion failed".into(), strict: true },
                    };
                    let mut all_rules = rules.write();
                    all_rules[index].draft.action = RefinedRuleAction::Native;
                    all_rules[index].draft.assertions.push(draft);
                },
                option { value: "", "＋ Add assertion…" }
                option { value: "option", "Assert a NixOS option value" }
                option { value: "packages", "Assert packages installed" }
                option { value: "custom", "Custom nix expression" }
            }
        }
    }
}

#[component]
fn EvidenceSection(rules: Signal<Vec<RefinedStigRule>>, index: usize) -> Element {
    let count = rules.read()[index].draft.evidence_requirements.len();
    rsx! {
        section { class: "refine-section",
            div { class: "refine-section-label", "Evidence for ATO " span { class: "refine-count", "· {count}" } }
            p { class: "refine-helper", "Artifacts collected at deploy and runtime to prove the control to an assessor. Seeded from the STIG check." }
            for (evidence_index, evidence) in rules.read()[index].draft.evidence_requirements.iter().cloned().enumerate() {
                EvidenceEditor { rules, index, evidence_index, evidence }
            }
            div { class: "refine-evidence-actions",
                button { class: "btn btn-ghost focus-ring", onclick: move |_| push_evidence(&mut rules, index, EvidenceRequirementDraft::Command { command: String::new(), expected_output: String::new() }), Icon { name: IconName::Plus, size: 10 } " Command output" }
                button { class: "btn btn-ghost focus-ring", onclick: move |_| push_evidence(&mut rules, index, EvidenceRequirementDraft::File { path: String::new(), expected_content: String::new() }), Icon { name: IconName::Plus, size: 10 } " File contents" }
                button { class: "btn btn-ghost focus-ring", onclick: move |_| push_evidence(&mut rules, index, EvidenceRequirementDraft::UnitState { unit: String::new(), state: "active".into() }), Icon { name: IconName::Plus, size: 10 } " systemd unit state" }
                button { class: "btn btn-ghost focus-ring", onclick: move |_| push_evidence(&mut rules, index, EvidenceRequirementDraft::Log { source: "journald".into(), unit: None, pattern: String::new() }), Icon { name: IconName::Plus, size: 10 } " Log excerpt" }
                button { class: "btn btn-ghost focus-ring", onclick: move |_| push_evidence(&mut rules, index, EvidenceRequirementDraft::Attestation { description: String::new() }), Icon { name: IconName::Plus, size: 10 } " Store-path attestation" }
            }
        }
    }
}

fn push_evidence(rules: &mut Signal<Vec<RefinedStigRule>>, index: usize, evidence: EvidenceRequirementDraft) {
    let mut all_rules = rules.write();
    if matches!(all_rules[index].draft.action, RefinedRuleAction::Unbound) {
        all_rules[index].draft.action = RefinedRuleAction::Manual;
    }
    all_rules[index].draft.evidence_requirements.push(evidence);
}

#[derive(Props, Clone, PartialEq)]
struct AssertionEditorProps {
    rules: Signal<Vec<RefinedStigRule>>,
    index: usize,
    assertion_index: usize,
    assertion: PolicyAssertionDraft,
}

#[component]
fn AssertionEditor(props: AssertionEditorProps) -> Element {
    let mut rules = props.rules;
    let index = props.index;
    let assertion_index = props.assertion_index;
    let remove = move |_| {
        rules.write()[index].draft.assertions.remove(assertion_index);
    };
    let failure = match &props.assertion {
        PolicyAssertionDraft::NixosOption { failure_message, .. }
        | PolicyAssertionDraft::PackagesInstalled { failure_message, .. }
        | PolicyAssertionDraft::CustomExpression { failure_message, .. } => failure_message.clone(),
    };
    rsx! {
        div { class: "refine-assertion-card",
            div { class: "refine-inferred-badge", "inferred · review" }
            div { class: "refine-editor-row",
            div { class: "refine-editor-content", match props.assertion.clone() {
                PolicyAssertionDraft::NixosOption { path, operator, expected_value, .. } => rsx! {
                    div { class: "refine-editor-title", Icon { name: IconName::File, size: 11 } " Assert a NixOS option value" }
                    div { class: "refine-option-row", input { class: "input focus-ring mono", placeholder: "services.openssh.settings.PermitRootLogin", value: "{path}", oninput: move |e| { if let PolicyAssertionDraft::NixosOption { path, .. } = &mut rules.write()[index].draft.assertions[assertion_index] { *path = e.value(); } } } select { class: "input focus-ring mono refine-operator", value: "{operator.as_str()}", onchange: move |e| { if let PolicyAssertionDraft::NixosOption { operator, .. } = &mut rules.write()[index].draft.assertions[assertion_index] { *operator = match e.value().as_str() { "!=" => ComparisonOperator::NotEqual, ">=" => ComparisonOperator::GreaterOrEqual, "<=" => ComparisonOperator::LessOrEqual, _ => ComparisonOperator::Equal }; } }, option { value: "==", "==" } option { value: "!=", "!=" } option { value: ">=", "≥" } option { value: "<=", "≤" } } input { class: "input focus-ring mono refine-expected", placeholder: "true", value: "{typed_value_text(&expected_value)}", oninput: move |e| { if let PolicyAssertionDraft::NixosOption { expected_value, .. } = &mut rules.write()[index].draft.assertions[assertion_index] { *expected_value = TypedPolicyValue::String(e.value()); } } } }
                    div { class: "mono refine-expression-hint", "→ config.{path} {operator.as_str()} {typed_value_text(&expected_value)}" }
                    input { class: "input focus-ring refine-failure", placeholder: "Failure message shown when assertion fails", value: "{failure}", oninput: move |e| { set_assertion_failure(&mut rules, index, assertion_index, e.value()); } }
                },
                PolicyAssertionDraft::PackagesInstalled { packages, .. } => {
                    let packages_text = packages.join(", ");
                    rsx! {
                        div { class: "refine-editor-title", Icon { name: IconName::File, size: 11 } " Assert these packages are in the system closure" }
                        input { class: "input focus-ring mono", placeholder: "openssh, auditd, aide", value: "{packages_text}", oninput: move |e| { if let PolicyAssertionDraft::PackagesInstalled { packages, .. } = &mut rules.write()[index].draft.assertions[assertion_index] { *packages = e.value().split(',').map(|part| part.trim().to_string()).filter(|part| !part.is_empty()).collect(); } } }
                        div { class: "mono refine-expression-hint", "→ builtins.any (p: p.pname == \"…\") config.environment.systemPackages" }
                        input { class: "input focus-ring refine-failure", placeholder: "Failure message shown when assertion fails", value: "{failure}", oninput: move |e| { set_assertion_failure(&mut rules, index, assertion_index, e.value()); } }
                    }
                },
                PolicyAssertionDraft::CustomExpression { expression, .. } => rsx! {
                    div { class: "refine-editor-title", Icon { name: IconName::Terminal, size: 11 } " Custom nix expression (must evaluate to " span { class: "mono", "true" } ")" }
                    textarea { class: "input focus-ring mono", rows: 2, placeholder: "cfg.config.networking.firewall.enable == true", value: "{expression}", oninput: move |e| { if let PolicyAssertionDraft::CustomExpression { expression, .. } = &mut rules.write()[index].draft.assertions[assertion_index] { *expression = e.value(); } } }
                    input { class: "input focus-ring refine-failure", placeholder: "Failure message shown when assertion fails", value: "{failure}", oninput: move |e| { set_assertion_failure(&mut rules, index, assertion_index, e.value()); } }
                },
            } }
            button { class: "btn-icon focus-ring refine-remove", aria_label: "Remove assertion", title: "Remove", onclick: remove, Icon { name: IconName::X, size: 13 } }
            }
        }
    }
}

fn set_assertion_failure(rules: &mut Signal<Vec<RefinedStigRule>>, index: usize, assertion_index: usize, value: String) {
    match &mut rules.write()[index].draft.assertions[assertion_index] {
        PolicyAssertionDraft::NixosOption { failure_message, .. }
        | PolicyAssertionDraft::PackagesInstalled { failure_message, .. }
        | PolicyAssertionDraft::CustomExpression { failure_message, .. } => *failure_message = value,
    }
}

#[derive(Props, Clone, PartialEq)]
struct EvidenceEditorProps {
    rules: Signal<Vec<RefinedStigRule>>,
    index: usize,
    evidence_index: usize,
    evidence: EvidenceRequirementDraft,
}

#[component]
fn EvidenceEditor(props: EvidenceEditorProps) -> Element {
    let mut rules = props.rules;
    let index = props.index;
    let evidence_index = props.evidence_index;
    let remove = move |_| {
        rules.write()[index].draft.evidence_requirements.remove(evidence_index);
    };
    let kind = match &props.evidence {
        EvidenceRequirementDraft::Command { .. } => "command",
        EvidenceRequirementDraft::File { .. } => "file",
        EvidenceRequirementDraft::UnitState { .. } => "unit state",
        EvidenceRequirementDraft::Log { .. } => "log",
        EvidenceRequirementDraft::Attestation { .. } => "attestation",
    };
    rsx! {
        div { class: "refine-evidence-card",
            div { class: "refine-evidence-head", span { class: "chip chip-unknown", "{kind}" } span { class: "refine-identity-spacer" } button { class: "btn-icon focus-ring refine-remove", aria_label: "Remove evidence", title: "Remove", onclick: remove, Icon { name: IconName::X, size: 13 } } }
            match props.evidence.clone() {
                EvidenceRequirementDraft::Command { command, expected_output } => rsx! { label { class: "refine-evidence-label", "command" } input { class: "input focus-ring mono refine-evidence-input", placeholder: "sshd -T | grep …", value: "{command}", oninput: move |e| { if let EvidenceRequirementDraft::Command { command, .. } = &mut rules.write()[index].draft.evidence_requirements[evidence_index] { *command = e.value(); } } } label { class: "refine-evidence-label", "expected output" } input { class: "input focus-ring mono refine-evidence-input", placeholder: "compliant", value: "{expected_output}", oninput: move |e| { if let EvidenceRequirementDraft::Command { expected_output, .. } = &mut rules.write()[index].draft.evidence_requirements[evidence_index] { *expected_output = e.value(); } } } },
                EvidenceRequirementDraft::File { path, expected_content } => rsx! { label { class: "refine-evidence-label", "path" } input { class: "input focus-ring mono refine-evidence-input", placeholder: "/etc/issue", value: "{path}", oninput: move |e| { if let EvidenceRequirementDraft::File { path, .. } = &mut rules.write()[index].draft.evidence_requirements[evidence_index] { *path = e.value(); } } } label { class: "refine-evidence-label", "must contain" } textarea { class: "input focus-ring mono refine-evidence-input", rows: 2, placeholder: "banner text", value: "{expected_content}", oninput: move |e| { if let EvidenceRequirementDraft::File { expected_content, .. } = &mut rules.write()[index].draft.evidence_requirements[evidence_index] { *expected_content = e.value(); } } } },
                EvidenceRequirementDraft::UnitState { unit, state } => rsx! { label { class: "refine-evidence-label", "unit" } input { class: "input focus-ring mono refine-evidence-input", placeholder: "auditd.service", value: "{unit}", oninput: move |e| { if let EvidenceRequirementDraft::UnitState { unit, .. } = &mut rules.write()[index].draft.evidence_requirements[evidence_index] { *unit = e.value(); } } } label { class: "refine-evidence-label", "state" } input { class: "input focus-ring mono refine-evidence-input", placeholder: "active", value: "{state}", oninput: move |e| { if let EvidenceRequirementDraft::UnitState { state, .. } = &mut rules.write()[index].draft.evidence_requirements[evidence_index] { *state = e.value(); } } } },
                EvidenceRequirementDraft::Log { source, unit, pattern } => rsx! { label { class: "refine-evidence-label", "unit" } input { class: "input focus-ring mono refine-evidence-input", placeholder: "auditd.service", value: "{unit.clone().unwrap_or_default()}", oninput: move |e| { if let EvidenceRequirementDraft::Log { unit, .. } = &mut rules.write()[index].draft.evidence_requirements[evidence_index] { *unit = (!e.value().trim().is_empty()).then(|| e.value()); } } } label { class: "refine-evidence-label", "log line matches" } input { class: "input focus-ring mono refine-evidence-input", placeholder: "audit: rules loaded", value: "{pattern}", oninput: move |e| { if let EvidenceRequirementDraft::Log { pattern, .. } = &mut rules.write()[index].draft.evidence_requirements[evidence_index] { *pattern = e.value(); } } } input { type: "hidden", value: "{source}" } },
                EvidenceRequirementDraft::Attestation { description } => rsx! { label { class: "refine-evidence-label", "agent reports" } textarea { class: "input focus-ring mono refine-evidence-input", rows: 2, placeholder: "booted generation / store-path hash", value: "{description}", oninput: move |e| { if let EvidenceRequirementDraft::Attestation { description } = &mut rules.write()[index].draft.evidence_requirements[evidence_index] { *description = e.value(); } } } },
            }
        }
    }
}

fn typed_value_text(value: &TypedPolicyValue) -> String {
    match value {
        TypedPolicyValue::Boolean(value) => value.to_string(),
        TypedPolicyValue::Integer(value) | TypedPolicyValue::String(value) | TypedPolicyValue::List(value) | TypedPolicyValue::AttributeSet(value) => value.clone(),
        TypedPolicyValue::Null => "null".to_string(),
    }
}

fn slugify(value: &str) -> String { let mut out = String::new(); for c in value.chars() { if c.is_ascii_alphanumeric() { out.push(c.to_ascii_lowercase()); } else if !out.ends_with('-') { out.push('-'); } } out.trim_matches('-').to_string() }

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn source_and_local_fields_are_independent() { let rule = RefinedStigRule { source: SourceStigRule { rule_id: "V-1".into(), group_id: None, stig_id: Some("V-1".into()), title: Some("Official".into()), description: Some("Source".into()), source_severity: Some("high".into()), fix_text: Some("Fix".into()), checks: vec![], identifiers: vec![], references: vec![], platforms: vec![], rule_order: 0 }, draft: RefinedPolicyDraft { local_name: "Local".into(), local_description: "Local description".into(), local_severity: "low".into(), local_rationale: "Local rationale".into(), implementation_note: String::new(), action: RefinedRuleAction::Unbound, assertion_mode: "all".into(), assertions: vec![], evidence_requirements: vec![] }, selected: true }; assert_eq!(rule.source.source_severity.as_deref(), Some("high")); assert_eq!(rule.source.fix_text.as_deref(), Some("Fix")); assert_eq!(rule.source.description.as_deref(), Some("Source")); }
    #[test]
    fn slugify_normalizes_punctuation() { assert_eq!(slugify("V-268/089 (STIG)"), "v-268-089-stig"); }
    #[test]
    fn source_identity_prefers_vulnerability_group_over_rule_id() {
        let source = SourceStigRule { rule_id: "SV-268089r1_rule".into(), group_id: Some("V-268089".into()), stig_id: None, title: None, description: None, source_severity: None, fix_text: None, checks: vec![], identifiers: vec![], references: vec![], platforms: vec![], rule_order: 0 };
        assert_eq!(source_identity(&source), "V-268089");
    }
    #[test]
    fn package_assertion_serializes_required_package_names() {
        let assertion = PolicyAssertionDraft::PackagesInstalled { packages: vec!["openssh".into(), "auditd".into()], failure_message: "missing".into(), strict: true };
        let rule = assertion.to_rule(1);
        assert!(rule.expression.contains("[ \"openssh\" \"auditd\" ]"));
        assert!(rule.expression.contains("builtins.all (required:"));
    }
}
