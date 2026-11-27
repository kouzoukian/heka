use crate::desktop::{Chord, Key};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyStroke {
    pub key: Key,
    pub down: bool,
}

pub trait KeyListener {
    fn heard(&mut self) -> Vec<KeyStroke>;

    fn has_alt_gr(&self) -> bool;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Captured {
    Chord(Chord),
    Cancelled,
}

#[derive(Debug, Default)]
pub struct KeyCapture {
    alt_gr: bool,
    down: Vec<Key>,
    chosen: Option<Captured>,
}

impl KeyCapture {
    pub fn new(alt_gr: bool) -> Self {
        Self {
            alt_gr,
            ..Self::default()
        }
    }

    pub fn hear(&mut self, stroke: KeyStroke) -> Option<Captured> {
        let KeyStroke { key, down } = stroke;
        let was_down = self.down.contains(&key);
        if down {
            if !was_down {
                let added_by_windows = self.alt_gr
                    && key == Key::RIGHT_ALT
                    && self.down.last() == Some(&Key::LEFT_CTRL);
                if added_by_windows {
                    self.down.pop();
                }
                if !key.is_modifier() {
                    self.chosen.get_or_insert_with(|| {
                        if key == Key::ESCAPE && self.down.is_empty() {
                            Captured::Cancelled
                        } else {
                            combination(&self.down, key)
                        }
                    });
                }
                self.down.push(key);
            }
        } else if was_down || key == Key::PRINT_SCREEN {
            self.down.retain(|&held| held != key);
            self.chosen
                .get_or_insert_with(|| combination(&self.down, key));
        }
        if self.down.is_empty() {
            self.chosen.take()
        } else {
            None
        }
    }
}

fn combination(held: &[Key], key: Key) -> Captured {
    let mut keys: Vec<Key> = held
        .iter()
        .copied()
        .filter(|held| held.is_modifier())
        .collect();
    keys.push(key);
    Captured::Chord(Chord::new(&keys).expect("only modifiers come before the key"))
}
