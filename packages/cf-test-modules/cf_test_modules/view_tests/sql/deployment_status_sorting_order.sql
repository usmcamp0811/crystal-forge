-- Get all statuses in order to verify sorting
SELECT 
    status_display,
    count,
    ROW_NUMBER() OVER() as row_num
FROM view_deployment_status
ORDER BY 
    CASE status_display
        WHEN 'Up to Date' THEN 1
        WHEN 'Behind' THEN 2 
        WHEN 'Evaluation Failed' THEN 3
        WHEN 'No Evaluation' THEN 4
        WHEN 'No Deployment' THEN 5
        WHEN 'Never Seen' THEN 6
        WHEN 'Unknown' THEN 7
        ELSE 8
    END;
