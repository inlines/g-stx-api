// @generated automatically by Diesel CLI.

diesel::table! {
    covers (id) {
        id -> Uuid,
        product_id -> Uuid,
        image_url -> Text,
    }
}

diesel::table! {
    products (id) {
        id -> Uuid,
        name -> Text,
        summary -> Text,
        first_release_date -> Timestamp,
    }
}

diesel::table! {
    sales (id) {
        id -> Uuid,
        created_at -> Timestamp,
        product_id -> Uuid,
        total_price -> Numeric,
    }
}

diesel::joinable!(covers -> products (product_id));
diesel::joinable!(sales -> products (product_id));

diesel::allow_tables_to_appear_in_same_query!(
    covers,
    products,
    sales,
);
