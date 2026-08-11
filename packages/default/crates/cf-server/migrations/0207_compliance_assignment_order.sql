-- Preserve the declared order of assignment additions.

ALTER TABLE compliance_assignment_additions
    ADD COLUMN addition_order integer;

WITH ordered AS (
    SELECT assignment_version_id, policy_version_id,
           ROW_NUMBER() OVER (
               PARTITION BY assignment_version_id
               ORDER BY policy_version_id
           ) - 1 AS addition_order
    FROM compliance_assignment_additions
)
UPDATE compliance_assignment_additions additions
SET addition_order = ordered.addition_order
FROM ordered
WHERE additions.assignment_version_id = ordered.assignment_version_id
  AND additions.policy_version_id = ordered.policy_version_id;

ALTER TABLE compliance_assignment_additions
    ALTER COLUMN addition_order SET NOT NULL,
    ADD CONSTRAINT compliance_assignment_additions_order_unique
        UNIQUE (assignment_version_id, addition_order);
