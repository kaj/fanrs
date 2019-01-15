-- Note: orig lang and orig_episode should both be null or both non-null.
alter table episodes add column orig_lang varchar;
alter table episodes add column orig_episode varchar;
