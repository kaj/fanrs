alter table issues
alter column price type integer using price * 100;
