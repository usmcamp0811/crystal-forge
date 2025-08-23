SELECT EXISTS (
    SELECT 1 
    FROM information_schema.views 
    WHERE table_name = 'view_systems_status_table'
);
