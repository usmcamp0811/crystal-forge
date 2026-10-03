-- Apply to a disposable database migrated through 0296, then migrate to 0297.
-- Fixed UUIDs make the before/after lineage assertions independent of sorting.
INSERT INTO users(id,username,first_name,last_name,email)
VALUES ('00000000-0000-4000-8000-000000000091','ra-upgrade','Risk','Tester','ra-upgrade@example.invalid');
INSERT INTO environments(id,name) VALUES
 ('00000000-0000-4000-8000-000000000092','ra-upgrade-environment');
INSERT INTO systems(id,hostname,public_key,derivation,environment_id) VALUES
 ('00000000-0000-4000-8000-000000000093','ra-upgrade-host','ra-key','ra-key','00000000-0000-4000-8000-000000000092');
INSERT INTO cves(id) VALUES ('CVE-2099-12345');
INSERT INTO deployment_policies(id,name,policy_type,config,enabled) VALUES
 ('00000000-0000-4000-8000-000000000094','ra-policy','custom_check','{}',false);
INSERT INTO poam_findings(id,system_id,policy_lineage_id) VALUES
 ('00000000-0000-4000-8000-000000000095','00000000-0000-4000-8000-000000000093','00000000-0000-4000-8000-000000000094');
INSERT INTO finding_waivers(id,finding_id,justification,policy_version_id,observation_token,observation_snapshot,created_by)
VALUES ('00000000-0000-4000-8000-000000000101','00000000-0000-4000-8000-000000000095','first review',gen_random_uuid(),'observed','{}','00000000-0000-4000-8000-000000000091');
INSERT INTO finding_waivers(id,finding_id,justification,policy_version_id,observation_token,observation_snapshot,created_by,predecessor_id,predecessor_updated_at,review_due_at)
SELECT '00000000-0000-4000-8000-000000000102',finding_id,'renewed review',policy_version_id,observation_token,observation_snapshot,created_by,id,updated_at,current_date+90
FROM finding_waivers WHERE id='00000000-0000-4000-8000-000000000101';
INSERT INTO cve_system_dispositions(id,canonical_cve_id,canonical_package_name,system_id,state,justification,accepted_by,accepted_at,retired_at,retired_by,retirement_reason)
VALUES
 ('00000000-0000-4000-8000-000000000201','CVE-2099-12345','openssl','00000000-0000-4000-8000-000000000093','accepted','first','00000000-0000-4000-8000-000000000091',now(),now(),'00000000-0000-4000-8000-000000000091','renewed'),
 ('00000000-0000-4000-8000-000000000202','CVE-2099-12345','openssl','00000000-0000-4000-8000-000000000093','accepted','second','00000000-0000-4000-8000-000000000091',now(),now(),'00000000-0000-4000-8000-000000000091','renewed'),
 ('00000000-0000-4000-8000-000000000203','CVE-2099-12345','openssl','00000000-0000-4000-8000-000000000093','accepted','third','00000000-0000-4000-8000-000000000091',now(),NULL,NULL,NULL);
INSERT INTO cve_environment_dispositions(id,canonical_cve_id,canonical_package_name,environment_id,state,justification,accepted_by,accepted_at,retired_at,retired_by,retirement_reason)
VALUES
 ('00000000-0000-4000-8000-000000000301','CVE-2099-12345','openssl','00000000-0000-4000-8000-000000000092','accepted','first','00000000-0000-4000-8000-000000000091',now(),now(),'00000000-0000-4000-8000-000000000091','renewed'),
 ('00000000-0000-4000-8000-000000000302','CVE-2099-12345','openssl','00000000-0000-4000-8000-000000000092','accepted','second','00000000-0000-4000-8000-000000000091',now(),NULL,NULL,NULL);
INSERT INTO admin_audit_events(actor_user_id,actor_identifier,action,target,metadata)
VALUES
 ('00000000-0000-4000-8000-000000000091','ra-upgrade','cve_acceptance_renewed','host', '{"source_type":"host","predecessor_id":"00000000-0000-4000-8000-000000000201","successor_id":"00000000-0000-4000-8000-000000000202"}'),
 ('00000000-0000-4000-8000-000000000091','ra-upgrade','cve_acceptance_renewed','host', '{"source_type":"host","predecessor_id":"00000000-0000-4000-8000-000000000202","successor_id":"00000000-0000-4000-8000-000000000203"}'),
 ('00000000-0000-4000-8000-000000000091','ra-upgrade','cve_acceptance_renewed','environment', '{"source_type":"environment","predecessor_id":"00000000-0000-4000-8000-000000000301","successor_id":"00000000-0000-4000-8000-000000000302"}');
