ActiveRecord::Schema.define(version: 2024_01_01_000000) do
  create_table "posts", force: :cascade do |t|
    t.string "title", null: false
    t.text "body"
    t.integer "author_id"
    t.datetime "published_at"
    t.boolean "featured", default: false
    t.datetime "created_at", null: false
    t.datetime "updated_at", null: false
  end

  create_table "comments", force: :cascade do |t|
    t.string "body"
    t.integer "post_id"
    t.datetime "created_at", null: false
    t.datetime "updated_at", null: false
  end
end
