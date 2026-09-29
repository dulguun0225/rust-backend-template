create table t (
    id uuid primary key default uuidv7(),
    created_at timestamptz not null default now()
);
