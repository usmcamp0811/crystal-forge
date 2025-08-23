-- Test that the view returns proper structure
SELECT 
    health_status,
    count,
    LENGTH(health_status) as status_length
FROM view_fleet_health_status
LIMIT 5;
