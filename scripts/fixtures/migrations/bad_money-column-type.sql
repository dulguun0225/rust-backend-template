create table t (
    id uuid primary key default uuidv7(),
    total_amount numeric not null check (total_amount <> 'NaN'),
    total_currency char(3) not null
);
