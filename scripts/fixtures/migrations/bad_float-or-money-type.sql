create table t (
    id uuid primary key default uuidv7(),
    price double precision not null
);
