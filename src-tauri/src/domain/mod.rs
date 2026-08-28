//! Tipos e regras do domínio do NarraHub.
//!
//! Esta camada não conhece `rusqlite` nem `tauri`: ela existe para que uma
//! regra possa ser testada sem banco e sem app. Se um arquivo daqui precisar
//! importar um dos dois, a regra está no lugar errado.
//!
//! Os campos são `snake_case` e serializam com o mesmo nome — os modelos
//! TypeScript do frontend já espelham as colunas, e renomear para `camelCase`
//! só na fronteira Rust obrigaria a reescrever templates inteiros sem ganho.
//! Entrada de comando é a exceção documentada: ali o contrato é `camelCase`,
//! como já fazia `planning_save_card`.

pub mod ids;
pub mod planning;
pub mod universe;
pub mod workspace;
