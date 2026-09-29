create table greeting (
    id uuid primary key default uuidv7(),
    name text not null,
    created_at timestamptz not null,
    version bigint not null
);
create table audit (
    id uuid primary key default uuidv7(),
    note text not null
);
