use super::*;

#[test]
fn every_tab_is_open_by_default() {
    let documents = OpenDocuments::default();
    assert_eq!(documents.tabs(), Document::ALL.to_vec());
    assert!(documents.closed().is_empty());
    assert!(documents.saved().is_empty());
}

#[test]
fn closing_an_editor_drops_its_tab_and_keeps_strip_order() {
    let mut documents = OpenDocuments::default();
    assert!(documents.close(Document::UiEditor));
    assert!(!documents.close(Document::UiEditor), "already closed");
    assert_eq!(
        documents.tabs(),
        vec![Document::Viewport, Document::Scripts, Document::Animation]
    );
    assert!(documents.close(Document::Scripts));
    assert_eq!(
        documents.closed(),
        vec![Document::Scripts, Document::UiEditor],
        "the + menu lists them in strip order, not closing order"
    );
}

#[test]
fn the_place_tab_never_closes() {
    let mut documents = OpenDocuments::default();
    assert!(!documents.close(Document::Viewport));
    assert!(documents.is_open(Document::Viewport));
}

#[test]
fn opening_reports_whether_the_tab_came_back() {
    let mut documents = OpenDocuments::default();
    assert!(!documents.open(Document::Animation), "was never closed");
    documents.close(Document::Animation);
    assert!(documents.open(Document::Animation));
    assert!(documents.is_open(Document::Animation));
    assert_eq!(documents.tabs(), Document::ALL.to_vec());
}

#[test]
fn closed_editors_round_trip_through_their_saved_keys() {
    let mut documents = OpenDocuments::default();
    documents.close(Document::Animation);
    documents.close(Document::Scripts);
    let saved = documents.saved();
    assert_eq!(saved, vec!["animation".to_owned(), "scripts".to_owned()]);
    assert_eq!(
        OpenDocuments::restore(&saved).closed(),
        vec![Document::Scripts, Document::Animation]
    );
}

#[test]
fn a_hand_edited_list_cannot_hide_the_place_or_repeat_an_editor() {
    let saved = ["viewport", "ui_editor", "ui_editor", "bogus"].map(str::to_owned);
    let documents = OpenDocuments::restore(&saved);
    assert_eq!(documents.closed(), vec![Document::UiEditor]);
    assert!(documents.is_open(Document::Viewport));
}
