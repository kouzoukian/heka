use heka_app::text;

#[test]
fn the_delete_question_says_how_many_controllers_go_back_to_the_default_profile() {
    assert_eq!(
        text::delete_profile_question("Racing", 0),
        "Delete Racing? No Controller uses it."
    );
    assert_eq!(
        text::delete_profile_question("Racing", 1),
        "Delete Racing? 1 Controller goes back to the Default Profile."
    );
    assert_eq!(
        text::delete_profile_question("Racing", 3),
        "Delete Racing? 3 Controllers go back to the Default Profile."
    );
}
