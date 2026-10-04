# Design Specification

* [Exact-CVE evidence authority and CVE inventory reads](exact-cve-evidence-authority-and-inventory-reads.md) - Specifies how retained deployed-generation lineage authorizes exact-CVE POA&M verification, how fleet CVE reads choose exact versus legacy authority, and the contract of the /cves, /cve-inventory, and /cve-inventory-page routes.
* [Exact-CVE writer locking and fleet triage transactions](exact-cve-writer-locking-and-fleet-triage.md) - Defines the READ COMMITTED lock hierarchy for exact-CVE POA&M and deployment-state writers, the SQLSTATE 40001 retry rule, append-only environment dispositions, and the all-or-nothing fleet triage transaction.

# Operator Guide

* [Fleet CVE Triage Operator Guide](fleet-cve-triage.md) - Explains how operators triage fleet CVEs by environment (current, scheduled and historical inventory relations, OPEN/ACCEPTED/SCHEDULED dispositions, batch triage, risk acceptance register, conflicts and request bounds); open it before changing or using the CVE triage drawer.
