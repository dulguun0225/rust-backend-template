alter table invoice add column fee_amount numeric(12,2) not null check (fee_amount <> 'NaN'), add column fee_currency char(3) not null;
