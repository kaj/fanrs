alter table episodes rename column name to episode;
alter table episodes rename column title_id to title;
alter table episodes rename column orig_mag_id to orig_mag;
alter table episode_parts rename column episode_id to episode;
alter table articles_by rename column creator_alias_id to by_id;
alter table covers_by rename column creator_alias_id to by_id;
alter table episodes_by rename column creator_alias_id to by_id;
alter table publications rename column issue_id to issue;
