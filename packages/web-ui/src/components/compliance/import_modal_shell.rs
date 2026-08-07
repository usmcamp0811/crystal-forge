use dioxus::prelude::*;

#[derive(Props, Clone, PartialEq)]
pub struct ImportWorkflowModalProps {
    pub children: Element,
}

/// Shared sizing/scroll contract for the XCCDF import workflow states.
#[component]
pub fn ImportWorkflowModal(props: ImportWorkflowModalProps) -> Element {
    rsx! {
        div { class: "import-workflow-modal", {props.children} }
    }
}
