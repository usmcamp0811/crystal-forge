-- Get all health statuses in order to verify sorting
SELECT 
    health_status,
    count,
    ROW_NUMBER() OVER() as row_num
FROM view_fleet_health_status
ORDER BY 
    CASE health_status
        WHEN 'Healthy' THEN 1
        WHEN 'Warning' THEN 2
        WHEN 'Critical' THEN 3
        WHEN 'Offline' THEN 4
        ELSE 5
    END;
