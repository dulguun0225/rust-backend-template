create table invoice (
    id                uuid primary key default uuidv7(),
    total_amount      numeric(19,4) not null check (total_amount <> 'NaN'),
    total_currency    char(3) not null,
    issued_at         timestamptz not null
);
