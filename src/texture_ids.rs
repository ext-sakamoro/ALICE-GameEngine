//! Texture ids of the 2D renderer: id 0 is the renderer's own white texture, `texture()` returns
//! 1, 2, ... in load order, and `destroy()` releases them all. This module words the refusal of an
//! id that is not a loaded texture (compiled on every target, so it is tested natively).

/// The error text for texture id `tex` when the renderer holds `held` textures (the white one
/// included; 0 after `destroy()`).
#[must_use]
pub fn unknown_texture_message(tex: u32, held: usize) -> String {
    match held {
        0 => format!("sprite: texture id {tex}: the renderer was destroyed"),
        1 => format!("sprite: texture id {tex}: no texture was loaded with texture()"),
        2 => format!("sprite: texture id {tex} was not returned by texture() (the only id is 1)"),
        n => format!(
            "sprite: texture id {tex} was not returned by texture() (ids 1 to {})",
            n - 1
        ),
    }
}

/// Whether `tex` names a texture loaded with `texture()` when the renderer holds `held`.
#[must_use]
pub const fn is_loaded(tex: u32, held: usize) -> bool {
    tex != 0 && (tex as usize) < held
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_message_names_the_ids_that_exist() {
        assert_eq!(
            unknown_texture_message(3, 0),
            "sprite: texture id 3: the renderer was destroyed"
        );
        assert_eq!(
            unknown_texture_message(1, 1),
            "sprite: texture id 1: no texture was loaded with texture()"
        );
        assert_eq!(
            unknown_texture_message(0, 2),
            "sprite: texture id 0 was not returned by texture() (the only id is 1)"
        );
        assert_eq!(
            unknown_texture_message(9, 4),
            "sprite: texture id 9 was not returned by texture() (ids 1 to 3)"
        );
        for held in 0..6 {
            assert!(!unknown_texture_message(7, held).contains("1..0"));
        }
    }

    #[test]
    fn loaded_ids_are_1_to_held_minus_1() {
        assert!(
            !is_loaded(0, 3),
            "the white texture is not a loaded texture"
        );
        assert!(is_loaded(1, 3) && is_loaded(2, 3));
        assert!(!is_loaded(3, 3));
        assert!(
            !is_loaded(1, 0) && !is_loaded(1, 1),
            "destroyed or nothing loaded"
        );
    }
}
