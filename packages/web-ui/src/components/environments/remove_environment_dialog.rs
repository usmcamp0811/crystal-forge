//! Reference-style remove environment confirmation.

use dioxus::prelude::*;

use super::EnvironmentItem;
use crate::components::icon::{Icon, IconName};

#[derive(Props, Clone, PartialEq)]
pub struct RemoveEnvironmentDialogProps {
    pub environment: EnvironmentItem,
    pub on_cancel: EventHandler<()>,
    pub on_confirm: EventHandler<()>,
}

#[component]
pub fn RemoveEnvironmentDialog(props: RemoveEnvironmentDialogProps) -> Element {
    let env = props.environment.clone();
    let mut typed = use_signal(String::new);
    let matches = typed() == env.name;
    let has_systems = env.system_count > 0;
    let system_plural = if env.system_count == 1 { "" } else { "s" };

    rsx! {
        div { class: "modal-backdrop", onclick: move |_| props.on_cancel.call(()),
            div { class: "modal", style: "width:min(520px,96vw);", onclick: |evt| evt.stop_propagation(),
                div { class: "modal-head", style: "background:rgba(248,113,113,0.06);",
                    h2 { style: "color:#fecaca; display:flex; align-items:center; gap:8px;",
                        Icon { name: IconName::Warn, size: 16 }
                        "Remove environment"
                    }
                    p { "This removes the " span { class: "mono", style: "font-weight:600;", "{env.name}" } " environment." }
                }
                div { class: "modal-body",
                    if has_systems {
                        div { class: "sd-callout sd-callout-danger", style: "margin-bottom:12px;",
                            Icon { name: IconName::Warn, size: 14 }
                            div { style: "font-size:12px; color:#fecaca;",
                                strong { "{env.system_count} system{system_plural} still assigned to this environment." }
                                " Reassign them before removing."
                            }
                        }
                    }
                    div { class: "field",
                        label { "Type " span { class: "mono", style: "color:#fecaca; font-weight:700;", "{env.name}" } " to confirm" }
                        input {
                            class: "input focus-ring mono",
                            placeholder: "{env.name}",
                            value: "{typed}",
                            disabled: has_systems,
                            style: if !typed().is_empty() && !matches { "border-color:rgba(248,113,113,0.5);" } else { "" },
                            oninput: move |evt| typed.set(evt.value())
                        }
                    }
                }
                div { class: "modal-foot",
                    button { class: "btn btn-ghost focus-ring", onclick: move |_| props.on_cancel.call(()), "Cancel" }
                    button {
                        class: "btn focus-ring",
                        disabled: !matches || has_systems,
                        style: if matches && !has_systems { "background:#dc2626; color:white;" } else { "background:var(--cf-subtle-bg); color:var(--cf-text-muted);" },
                        onclick: move |_| props.on_confirm.call(()),
                        Icon { name: IconName::X, size: 13 }
                        " Remove environment"
                    }
                }
            }
        }
    }
}
