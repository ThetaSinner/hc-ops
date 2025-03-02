create table agent_tag
(
    agent blob primary key not null,
    tag   text unique not null
);
