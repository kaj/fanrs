-- Your SQL goes here

alter table issues add column ord int;
alter table issues add constraint issues_ord unique (ord);
