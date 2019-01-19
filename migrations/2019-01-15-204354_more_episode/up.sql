-- Note: orig lang and orig_episode should both be null or both non-null.
alter table episodes add column orig_lang varchar;
alter table episodes add column orig_episode varchar;
alter table episodes add column orig_date date;
alter table episodes add column orig_to_date date;
alter table episodes add column orig_sundays bool not null default false;
