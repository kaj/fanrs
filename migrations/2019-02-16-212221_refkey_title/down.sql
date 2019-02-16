alter table refkeys alter column title drop not null;
alter table refkeys alter column title set data type varchar(100);
