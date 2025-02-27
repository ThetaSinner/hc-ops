create table addr_tag
(
    tag     text not null primary key,
    address text not null,
    port    int  not null
);
