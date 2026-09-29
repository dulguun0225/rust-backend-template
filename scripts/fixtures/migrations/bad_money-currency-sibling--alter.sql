alter table invoice add column tax_amount numeric(19,4) not null check (tax_amount <> 'NaN');
