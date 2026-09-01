//! Fronteira do core com o mundo de fora. Hoje só o Tauri; o dia em que
//! houver uma CLI ou um servidor, ele entra aqui e não dentro da aplicação.

pub mod command_placement;
pub mod tauri;
