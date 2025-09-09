use std::sync::Arc;

#[allow(dead_code)]
pub fn contains_response_of_type<T>(responses: &[Arc<T>], variant: &T) -> bool {
    responses
        .iter()
        .any(|msg| std::mem::discriminant(&**msg) == std::mem::discriminant(variant))
}
