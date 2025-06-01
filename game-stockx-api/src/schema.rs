table! {
    products (id) {
      id -> Uuid,
      name -> Text,
      summary -> Text,
      first_release_date -> Text,
      cover -> Text,
    }
}

table! {
    sales (id) {
        id -> Uuid,
        created_at -> Timestamp,
        sum -> Integer,
    }
}

table! {
  covers (id) {
    id -> Uuid,
    url -> Text,
  }
}

joinable!(sales -> products (id));
joinable!(covers -> products (id));
allow_tables_to_appear_in_same_query!(
    products,
    sales,
    covers,
);