ALTER TABLE workspaces ADD COLUMN sort_order INTEGER NOT NULL DEFAULT 0;

ALTER TABLE connections ADD COLUMN sort_order INTEGER NOT NULL DEFAULT 0;

WITH ranked_workspaces AS (
    SELECT
        id,
        ROW_NUMBER() OVER (
            ORDER BY updated_at DESC, id ASC
        ) - 1 AS row_num
    FROM workspaces
)
UPDATE workspaces
SET sort_order = (
    SELECT row_num
    FROM ranked_workspaces
    WHERE ranked_workspaces.id = workspaces.id
);

WITH ranked_connections AS (
    SELECT
        id,
        ROW_NUMBER() OVER (
            PARTITION BY workspace_id
            ORDER BY updated_at DESC, id ASC
        ) - 1 AS row_num
    FROM connections
)
UPDATE connections
SET sort_order = (
    SELECT row_num
    FROM ranked_connections
    WHERE ranked_connections.id = connections.id
);
