-- Test that the view returns proper structure
SELECT 
    count,
    status_display,
    LENGTH(status_display) as display_length
FROM view_deployment_status
LIMIT 3;
