alter table issues
alter column price type decimal(5, 2) using price::decimal / 100;
