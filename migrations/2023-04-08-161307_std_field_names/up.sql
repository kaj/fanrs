alter table episodes rename column episode to name;
alter table episodes rename column title to title_id;
alter table episodes rename column orig_mag to orig_mag_id;
alter table episode_parts rename column episode to episode_id;
alter table articles_by rename column by_id to creator_alias_id;
alter table covers_by rename column by_id to creator_alias_id;
alter table episodes_by rename column by_id to creator_alias_id;
alter table publications rename column issue to issue_id;
