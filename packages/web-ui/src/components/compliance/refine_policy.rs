use dioxus::prelude::*;

use crate::api::models::{
    ImportedCustomCheck, ImportedCustomCheckRule, ImportedEvidenceRequirement,
    ImportedPolicyCustomization, XccdfRuleImportAction,
};
use crate::components::icon::{Icon, IconName};

#[derive(Clone, PartialEq, Debug)]
pub struct SourceCheck { pub system: String, pub content: String }

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
                let checks = packages.iter().map(|p| format!("(x.pname or \"\") == \"{}\"", p.replace('"', "\\\""))).collect::<Vec<_>>().join(" || ");
                ImportedCustomCheckRule { field_name: format!("packagesInstalled{index}"), expression: format!("builtins.all (p: builtins.any (x: {checks}) cfg.config.environment.systemPackages) [ ]"), description: failure_message.clone(), strict: *strict }
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
pub struct RefinePolicyStepProps { pub rules: Signal<Vec<RefinedStigRule>>, pub cursor: Signal<usize>, pub on_back: EventHandler<()>, pub on_finish: EventHandler<()> }

#[component]
pub fn RefinePolicyStep(mut props: RefinePolicyStepProps) -> Element {
    let selected: Vec<usize> = props.rules.read().iter().enumerate().filter_map(|(i, r)| r.selected.then_some(i)).collect();
    let position = (*props.cursor.read()).min(selected.len().saturating_sub(1));
    let Some(index) = selected.get(position).copied() else { return rsx! { div {} }; };
    let rule = props.rules.read()[index].clone();
    let source_id = rule.source.stig_id.clone().unwrap_or_else(|| rule.source.rule_id.clone());
    let percent = ((position + 1) as f32 / selected.len().max(1) as f32 * 100.0).round();
    rsx! {
        div { class: "modal-head", style: "flex:0 0 auto;", div { style: "display:flex;justify-content:space-between;align-items:center;", h2 { style: "display:flex;gap:8px;align-items:center;", Icon { name: IconName::Shield, size: 15 }, "Refine policy {position + 1} of {selected.len()}" }, span { class: "mono", "{source_id}" } }, div { style: "height:4px;background:var(--cf-divider);margin-top:9px;", div { style: "height:100%;width:{percent}%;background:var(--cf-brand-purple);" } } }
        div { class: "modal-body", style: "overflow:auto;flex:1 1 auto;min-height:0;", 
            div { style: "display:flex;gap:8px;align-items:center;margin-bottom:12px;", span { class: "chip chip-info", "Security & hardening" }, if let Some(group) = rule.source.group_id.as_ref() { span { class: "mono", "{group}" } }, div { style: "flex:1;" }, span { class: "mono", style: "font-size:10px;", "policy id: {slugify(&source_id)}" } }
            SourceStigCard { rule: rule.source.clone() }
            div { style: "display:grid;grid-template-columns:minmax(0,1fr) auto;gap:12px;align-items:end;", div { class: "field", label { "Policy name" }, input { class: "input focus-ring mono", value: "{rule.draft.local_name}", oninput: move |e| { props.rules.write()[index].draft.local_name = e.value(); } } }, div { class: "field", label { "Severity" }, div { class: "seg", for (value, label) in [("high","CAT I"),("medium","CAT II"),("low","CAT III")] { button { class: if rule.draft.local_severity == value { "active" } else { "" }, onclick: move |_| { props.rules.write()[index].draft.local_severity = value.into(); }, "{label}" } } } } }
            details { style: "margin:12px 0;", summary { style: "font-size:11px;color:var(--cf-text-muted);", "Additional policy details" }, div { style: "display:grid;gap:8px;margin-top:8px;", textarea { class: "input focus-ring", rows: 2, placeholder: "Description", value: "{rule.draft.local_description}", oninput: move |e| { props.rules.write()[index].draft.local_description = e.value(); } }, textarea { class: "input focus-ring", rows: 2, placeholder: "Rationale", value: "{rule.draft.local_rationale}", oninput: move |e| { props.rules.write()[index].draft.local_rationale = e.value(); } }, textarea { class: "input focus-ring", rows: 2, placeholder: "Implementation note", value: "{rule.draft.implementation_note}", oninput: move |e| { props.rules.write()[index].draft.implementation_note = e.value(); } } } }
            ImplementationChoice { rules: props.rules, index }
            if matches!(rule.draft.action, RefinedRuleAction::Native) { AssertionSection { rules: props.rules, index } }
            if matches!(rule.draft.action, RefinedRuleAction::Native | RefinedRuleAction::Manual) { EvidenceSection { rules: props.rules, index } }
            if matches!(rule.draft.action, RefinedRuleAction::Unbound) { div { class: "sd-callout sd-callout-info", "This requirement will be imported without a Crystal Forge implementation." } }
            if matches!(rule.draft.action, RefinedRuleAction::Opaque) { div { class: "sd-callout sd-callout-info", "The original XCCDF rule and check content will be preserved without execution." } }
        }
        div { class: "modal-foot", style: "flex:0 0 auto;justify-content:space-between;", button { class: "btn btn-ghost", onclick: move |_| props.on_back.call(()), Icon { name: IconName::ArrowLeft, size: 13 }, " Previous" }, div { style: "display:flex;gap:8px;", button { class: "btn btn-ghost", style: "color:#f87171;", onclick: move |_| { props.rules.write()[index].selected = false; }, "Exclude" }, if position + 1 < selected.len() { button { class: "btn btn-primary", onclick: move |_| props.cursor.set(position + 1), "Next" } } else { button { class: "btn btn-primary", disabled: !props.rules.read().iter().filter(|r| r.selected).all(RefinedStigRule::is_valid), onclick: move |_| props.on_finish.call(()), "Create bundle + {selected.len()} policies" } } } }
    }
}

#[component]
fn SourceStigCard(rule: SourceStigRule) -> Element {
    let severity = rule.source_severity.as_deref().unwrap_or("medium");
    let cat = match severity { "high" => "CAT I", "low" => "CAT III", _ => "CAT II" };
    let stig_id = rule.stig_id.clone().unwrap_or_else(|| rule.rule_id.clone());
    let title = rule.title.clone().unwrap_or_else(|| rule.rule_id.clone());
    let fix_text = rule.fix_text.clone();
    let check_text = rule.checks.first().map(|check| check.content.clone());
    rsx! { div { class: "card", style: "padding:0;margin:14px 0;border-radius:10px;overflow:hidden;", div { style: "display:flex;gap:8px;padding:9px 12px;background:var(--cf-subtle-bg);border-bottom:1px solid var(--cf-divider);font-size:10px;font-weight:700;letter-spacing:.08em;", Icon { name: IconName::Shield, size: 12 }, "FROM THE STIG", div { style: "flex:1;" }, span { class: "chip chip-unknown", "{cat}" } }, div { style: "padding:12px;", div { class: "mono", style: "font-size:10px;color:var(--cf-text-muted);", "{stig_id}" }, div { style: "font-weight:650;", "{title}" }, if let Some(fix) = fix_text { div { style: "margin-top:10px;font-size:11px;white-space:pre-wrap;", strong { "Official fix" }, p { "{fix}" } } }, if let Some(check) = check_text { div { style: "margin-top:10px;font-size:11px;white-space:pre-wrap;", strong { "Official check" }, p { "{check}" } } } } } }
}

#[component]
fn ImplementationChoice(rules: Signal<Vec<RefinedStigRule>>, index: usize) -> Element {
    let current = action_key(&rules.read()[index].draft.action);
    rsx! { div { style: "margin:14px 0;", label { style: "font-size:11px;font-weight:650;", "Implementation" }, div { style: "display:flex;gap:6px;flex-wrap:wrap;margin-top:6px;", for (key, label) in [("native","Native assertion"),("manual","Manual evidence"),("unbound","Unbound"),("opaque","Opaque"),("existing","Existing")] { button { class: if current == key { "btn btn-primary" } else { "btn btn-ghost" }, onclick: move |_| { set_action(&mut rules.write()[index].draft.action, key); }, "{label}" } } } } }
}
fn action_key(action: &RefinedRuleAction) -> &'static str { match action { RefinedRuleAction::Native => "native", RefinedRuleAction::Manual => "manual", RefinedRuleAction::Unbound => "unbound", RefinedRuleAction::Opaque => "opaque", RefinedRuleAction::Existing(_) => "existing" } }
fn set_action(action: &mut RefinedRuleAction, key: &str) { *action = match key { "native" => RefinedRuleAction::Native, "manual" => RefinedRuleAction::Manual, "opaque" => RefinedRuleAction::Opaque, "existing" => RefinedRuleAction::Existing(None), _ => RefinedRuleAction::Unbound }; }

#[component]
fn AssertionSection(rules: Signal<Vec<RefinedStigRule>>, index: usize) -> Element {
    let count = rules.read()[index].draft.assertions.len();
    rsx! { section { style: "border-top:1px solid var(--cf-divider);padding-top:12px;margin-top:12px;", div { style: "display:flex;align-items:center;gap:7px;", strong { "NixOS config assertions" }, span { class: "chip", style: "font-size:9px;color:var(--cf-brand-purple);", "EVAL-TIME" } }, p { style: "font-size:11px;color:var(--cf-text-muted);", "Asserted against the rendered config during Nix evaluation." }, if count == 0 { div { class: "sd-callout sd-callout-warn", style: "border-style:dashed;", "No assertion could be inferred from this STIG control. Add one below." } }, select { class: "input focus-ring", value: "", onchange: move |e| { let draft = match e.value().as_str() { "option" => PolicyAssertionDraft::NixosOption { path: String::new(), operator: ComparisonOperator::Equal, expected_value: TypedPolicyValue::String(String::new()), failure_message: "Option assertion failed".into(), strict: true }, "packages" => PolicyAssertionDraft::PackagesInstalled { packages: vec![], failure_message: "Required package is not installed".into(), strict: true }, _ => PolicyAssertionDraft::CustomExpression { field_name: format!("customAssertion{}", count + 1), expression: String::new(), failure_message: "Custom assertion failed".into(), strict: true } }; rules.write()[index].draft.assertions.push(draft); }, option { value: "", "＋ Add assertion…" }, option { value: "option", "Assert a NixOS option value" }, option { value: "packages", "Assert packages installed" }, option { value: "custom", "Custom nix expression" } } } }
}

#[component]
fn EvidenceSection(rules: Signal<Vec<RefinedStigRule>>, index: usize) -> Element {
    let count = rules.read()[index].draft.evidence_requirements.len();
    rsx! { section { style: "border-top:1px solid var(--cf-divider);padding-top:12px;margin-top:12px;", h3 { style: "font-size:12px;margin:0;", "Evidence for ATO · {count}" }, p { style: "font-size:11px;color:var(--cf-text-muted);", "Artifacts collected at deploy and runtime to prove the control to an assessor." }, select { class: "input focus-ring", value: "", onchange: move |e| { let item = match e.value().as_str() { "command" => EvidenceRequirementDraft::Command { command: String::new(), expected_output: String::new() }, "file" => EvidenceRequirementDraft::File { path: String::new(), expected_content: String::new() }, "unit" => EvidenceRequirementDraft::UnitState { unit: String::new(), state: "active".into() }, "log" => EvidenceRequirementDraft::Log { source: "journald".into(), unit: None, pattern: String::new() }, _ => EvidenceRequirementDraft::Attestation { description: String::new() } }; rules.write()[index].draft.evidence_requirements.push(item); }, option { value: "", "＋ Add evidence source…" }, option { value: "command", "Command output" }, option { value: "file", "File contents" }, option { value: "unit", "systemd unit state" }, option { value: "log", "Log excerpt" }, option { value: "attestation", "Store-path / signed attestation" } } } }
}

fn slugify(value: &str) -> String { let mut out = String::new(); for c in value.chars() { if c.is_ascii_alphanumeric() { out.push(c.to_ascii_lowercase()); } else if !out.ends_with('-') { out.push('-'); } } out.trim_matches('-').to_string() }

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn source_and_local_fields_are_independent() { let rule = RefinedStigRule { source: SourceStigRule { rule_id: "V-1".into(), group_id: None, stig_id: Some("V-1".into()), title: Some("Official".into()), description: Some("Source".into()), source_severity: Some("high".into()), fix_text: Some("Fix".into()), checks: vec![], identifiers: vec![], references: vec![], platforms: vec![], rule_order: 0 }, draft: RefinedPolicyDraft { local_name: "Local".into(), local_description: "Local description".into(), local_severity: "low".into(), local_rationale: "Local rationale".into(), implementation_note: String::new(), action: RefinedRuleAction::Unbound, assertion_mode: "all".into(), assertions: vec![], evidence_requirements: vec![] }, selected: true }; assert_eq!(rule.source.source_severity.as_deref(), Some("high")); assert_eq!(rule.source.fix_text.as_deref(), Some("Fix")); assert_eq!(rule.source.description.as_deref(), Some("Source")); }
    #[test]
    fn slugify_normalizes_punctuation() { assert_eq!(slugify("V-268/089 (STIG)"), "v-268-089-stig"); }
}
