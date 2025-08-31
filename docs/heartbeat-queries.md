# Database Relationships and Views Guide

## Understanding Multi-Table Relationships

When building database schemas with interconnected entities, you'll often need to create views that aggregate information from multiple related tables. This guide explains common patterns for handling these relationships.

## Core Relationship Patterns

### 1. One-to-Many with Latest Record Pattern

When you have a parent entity with multiple child records and need the most recent one:

```sql
-- Get the latest child record for each parent
WITH latest_children AS (
    SELECT DISTINCT ON (parent_id)
        parent_id,
        child_data,
        timestamp
    FROM child_table
    ORDER BY parent_id, timestamp DESC
)
SELECT
    p.id,
    p.name,
    lc.child_data,
    lc.timestamp AS last_updated
FROM parent_table p
LEFT JOIN latest_children lc ON p.id = lc.parent_id
```

**Key Points:**

- `DISTINCT ON` gets one record per grouping key
- `ORDER BY` must include the grouping key first, then the sorting criteria
- Use `LEFT JOIN` to include parents even without children

### 2. Multiple Activity Streams Pattern

When entities can be updated through different mechanisms (e.g., heartbeats, state changes, user actions):

```sql
-- Combine multiple activity sources
WITH activity_stream_1 AS (
    SELECT entity_id, timestamp AS last_activity
    FROM activity_table_1
    WHERE entity_id IS NOT NULL
),
activity_stream_2 AS (
    SELECT entity_id, timestamp AS last_activity
    FROM activity_table_2
    WHERE entity_id IS NOT NULL
),
combined_activity AS (
    SELECT
        entity_id,
        GREATEST(
            COALESCE(a1.last_activity, '1970-01-01'::timestamp),
            COALESCE(a2.last_activity, '1970-01-01'::timestamp)
        ) AS most_recent_activity
    FROM entities e
    LEFT JOIN activity_stream_1 a1 ON e.id = a1.entity_id
    LEFT JOIN activity_stream_2 a2 ON e.id = a2.entity_id
)
```

**Key Points:**

- Use `GREATEST()` to find the most recent timestamp across sources
- `COALESCE()` handles NULL values by providing defaults
- Consider what constitutes "no activity" (often '1970-01-01' as epoch start)

### 3. Status Derivation from Multiple Sources

When entity status depends on data from several related tables:

```sql
-- Derive status from multiple conditions
SELECT
    e.id,
    e.name,
    CASE
        WHEN condition_1 IS NULL THEN 'never_initialized'
        WHEN condition_2 >= threshold_value THEN 'active'
        WHEN condition_3 BETWEEN min_val AND max_val THEN 'warning'
        ELSE 'inactive'
    END AS derived_status
FROM entities e
LEFT JOIN related_table_1 r1 ON e.id = r1.entity_id
LEFT JOIN related_table_2 r2 ON e.id = r2.entity_id
```

**Key Points:**

- Order `CASE` conditions from most specific to most general
- Handle NULL values explicitly
- Consider what each status means for downstream consumers

## Advanced Patterns

### 4. Hierarchical Status Rollup

When child entity status affects parent status:

```sql
WITH child_status_summary AS (
    SELECT
        parent_id,
        COUNT(*) as total_children,
        COUNT(*) FILTER (WHERE status = 'healthy') as healthy_count,
        COUNT(*) FILTER (WHERE status = 'error') as error_count
    FROM child_entities
    GROUP BY parent_id
)
SELECT
    p.id,
    p.name,
    CASE
        WHEN css.error_count > 0 THEN 'degraded'
        WHEN css.healthy_count = css.total_children THEN 'healthy'
        WHEN css.total_children IS NULL THEN 'no_children'
        ELSE 'mixed'
    END as overall_status
FROM parent_entities p
LEFT JOIN child_status_summary css ON p.id = css.parent_id
```

### 5. Time-Based Status Windows

When status depends on activity within specific time periods:

```sql
SELECT
    entity_id,
    CASE
        WHEN last_activity > NOW() - INTERVAL '5 minutes' THEN 'online'
        WHEN last_activity > NOW() - INTERVAL '1 hour' THEN 'idle'
        WHEN last_activity > NOW() - INTERVAL '24 hours' THEN 'stale'
        WHEN last_activity IS NOT NULL THEN 'offline'
        ELSE 'never_seen'
    END as connectivity_status,
    last_activity
FROM entity_activity_view
```

## View Design Principles

### Separation of Concerns

- Create focused views for specific business logic
- Avoid mixing unrelated status calculations
- Make views composable for complex dashboards

### Performance Considerations

- Use CTEs for readability, but consider materialized views for heavy queries
- Index columns used in `DISTINCT ON` and `ORDER BY` clauses
- Be mindful of cartesian products in multi-table joins

### Maintainability

- Use descriptive CTE names that explain their purpose
- Comment complex business logic
- Consider creating helper functions for repeated calculations

## Common Pitfalls

1. **Missing NULL Handling**: Always consider what happens when related data doesn't exist
2. **Incorrect JOIN Types**: Use LEFT JOIN when parent records should appear even without children
3. **Time Zone Issues**: Be consistent with timezone handling across time-based comparisons
4. **Performance**: `DISTINCT ON` can be expensive on large tables without proper indexing

## Testing Relationship Views

Create test cases that verify:

- Entities with no related records
- Entities with multiple related records
- Edge cases around time boundaries
- NULL value handling
- Expected vs actual status derivation

This approach ensures your views handle real-world data variations gracefully.
