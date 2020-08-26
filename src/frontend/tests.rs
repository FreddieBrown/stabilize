use super::*;

#[test]
fn test_no_filter() {
    let buffer = [0 as u8; 1024];
    match Filter::no_filter(&buffer){
        Some(_) => assert!(true),
        None => assert!(false)
    }
}

#[test]
fn test_all_filter() {
    let buffer = [0 as u8; 1024];
    match Filter::all_filter(&buffer){
        Some(_) => assert!(false),
        None => assert!(true)
    }
}