//! XCCDF 1.2 XML writer for CF-XCCDF bundle export.

use quick_xml::Writer;
use std::io::Cursor;

use super::super::digest::{BundleMembershipEntry, BundleVersionCanonical};

fn el(
    writer: &mut Writer<Cursor<&mut Vec<u8>>>,
    name: &str,
    text: &str,
) -> Result<(), std::io::Error> {
    writer.write_event(quick_xml::events::Event::Start(
        quick_xml::events::BytesStart::new(name),
    ))?;
    writer.write_event(quick_xml::events::Event::Text(
        quick_xml::events::BytesText::new(text),
    ))?;
    writer.write_event(quick_xml::events::Event::End(
        quick_xml::events::BytesEnd::new(name),
    ))
}

/// Write a complete XCCDF 1.2 Benchmark for a bundle version.
pub fn write_bundle_xccdf(canonical: &BundleVersionCanonical) -> Result<String, std::io::Error> {
    let mut buf = Vec::new();
    let mut writer = Writer::new(Cursor::new(&mut buf));

    writer.write_event(quick_xml::events::Event::Decl(
        quick_xml::events::BytesDecl::new("1.0", Some("UTF-8"), None),
    ))?;

    let benchmark_id = format!(
        "xccdf_org.crystalforge_benchmark_{}",
        canonical
            .members
            .first()
            .map(|m| m.policy_version_id.to_string().replace('-', ""))
            .unwrap_or_else(|| "unknown".into())
    );

    let mut bench = quick_xml::events::BytesStart::new("Benchmark");
    bench.push_attribute(("xmlns", "http://checklists.nist.gov/xccdf/1.2"));
    bench.push_attribute(("xmlns:cf", "urn:crystal-forge:xccdf:1"));
    bench.push_attribute(("id", benchmark_id.as_str()));
    writer.write_event(quick_xml::events::Event::Start(bench))?;

    el(&mut writer, "status", "draft")?;
    el(&mut writer, "title", &canonical.name)?;
    if let Some(ref desc) = canonical.description {
        el(&mut writer, "description", desc)?;
    }
    el(
        &mut writer,
        "version",
        canonical.framework_version.as_deref().unwrap_or("0.1.0"),
    )?;

    // Metadata
    writer.write_event(quick_xml::events::Event::Start(
        quick_xml::events::BytesStart::new("metadata"),
    ))?;
    writer.write_event(quick_xml::events::Event::Start(
        quick_xml::events::BytesStart::new("cf:bundle"),
    ))?;
    if let Some(ref fw_ver) = canonical.framework_version {
        let mut fw = quick_xml::events::BytesStart::new("cf:framework");
        fw.push_attribute(("name", canonical.framework.as_str()));
        fw.push_attribute(("version", fw_ver.as_str()));
        writer.write_event(quick_xml::events::Event::Empty(fw))?;
    }
    el(&mut writer, "cf:layer", &canonical.layer)?;
    el(&mut writer, "cf:owner", &canonical.owner)?;
    writer.write_event(quick_xml::events::Event::End(
        quick_xml::events::BytesEnd::new("cf:bundle"),
    ))?;
    writer.write_event(quick_xml::events::Event::End(
        quick_xml::events::BytesEnd::new("metadata"),
    ))?;

    // Baseline profile
    let prof_id = format!(
        "xccdf_org.crystalforge_profile_{}",
        if canonical.members.is_empty() {
            "empty"
        } else {
            "baseline"
        }
    );
    let mut prof = quick_xml::events::BytesStart::new("Profile");
    prof.push_attribute(("id", prof_id.as_str()));
    writer.write_event(quick_xml::events::Event::Start(prof))?;
    el(&mut writer, "title", "Crystal Forge Baseline")?;
    for member in &canonical.members {
        let rid = format!(
            "xccdf_org.crystalforge_rule_{}",
            member.policy_version_id.to_string().replace('-', "")
        );
        let mut sel = quick_xml::events::BytesStart::new("select");
        sel.push_attribute(("idref", rid.as_str()));
        sel.push_attribute(("selected", if member.selected { "true" } else { "false" }));
        writer.write_event(quick_xml::events::Event::Empty(sel))?;
    }
    writer.write_event(quick_xml::events::Event::End(
        quick_xml::events::BytesEnd::new("Profile"),
    ))?;

    // Rules
    for member in &canonical.members {
        let rid = format!(
            "xccdf_org.crystalforge_rule_{}",
            member.policy_version_id.to_string().replace('-', "")
        );
        let mut rule = quick_xml::events::BytesStart::new("Rule");
        rule.push_attribute(("id", rid.as_str()));
        rule.push_attribute(("selected", if member.selected { "true" } else { "false" }));
        writer.write_event(quick_xml::events::Event::Start(rule))?;
        el(
            &mut writer,
            "title",
            &format!("Policy version {}", member.policy_version_id),
        )?;
        writer.write_event(quick_xml::events::Event::End(
            quick_xml::events::BytesEnd::new("Rule"),
        ))?;
    }

    writer.write_event(quick_xml::events::Event::End(
        quick_xml::events::BytesEnd::new("Benchmark"),
    ))?;
    drop(writer);

    Ok(String::from_utf8(buf).unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn produces_valid_xml() {
        let id = Uuid::parse_str("11111111-0000-0000-0000-000000000001").unwrap();
        let canonical = BundleVersionCanonical {
            name: "Test Bundle".into(),
            framework: "STIG".into(),
            framework_version: Some("V1R1".into()),
            description: Some("A test bundle".into()),
            layer: "os".into(),
            owner: "Team".into(),
            members: vec![BundleMembershipEntry {
                policy_version_id: id,
                selected: true,
            }],
        };
        let xml = write_bundle_xccdf(&canonical).unwrap();
        assert!(xml.contains("<Benchmark"));
        assert!(xml.contains("xccdf_org.crystalforge_benchmark_"));
        assert!(xml.contains("<cf:bundle"));
    }
}
