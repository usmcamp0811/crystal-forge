-- Test that the view returns expected columns
SELECT 
    hostname,
    status,
    hours_ago,
    ip,
    version
FROM view_critical_systems
LIMIT 1;
