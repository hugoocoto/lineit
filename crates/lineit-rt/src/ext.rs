//! Modules that aren't IR: Lua modules ([`crate::lua`]) and Rust modules
//! from packages ([`crate::native`]). The interpreter and compiled programs
//! run both through this, by the names of their pins.

use crate::types::Type;
use crate::value::Value;

/// What a tick returns: the outputs it set (None for those it left), and
/// the tick outputs to fire, in order.
pub type Ticked = (Vec<Option<Value>>, Vec<String>);

pub trait Ext: Send {
    /// Makes the state of a square, with its PARAMS. Returns its number.
    fn instance(&mut self, params: &[(&str, Value)]) -> Result<usize, String>;

    /// The OUTPUTS (by name and type) of a pure square, from its INPUTS.
    fn eval(&mut self, inst: usize, inputs: &[(&str, Value)], outputs: &[(&str, &Type)]) -> Result<Vec<Value>, String>;

    /// Tick input INPUT fired (None: the program started).
    fn tick(
        &mut self,
        inst: usize,
        input: Option<&str>,
        inputs: &[(&str, Value)],
        outputs: &[(&str, &Type)],
    ) -> Result<Ticked, String>;
}

#[cfg(feature = "lua")]
impl Ext for crate::lua::LuaModule {
    fn instance(&mut self, params: &[(&str, Value)]) -> Result<usize, String> {
        crate::lua::LuaModule::instance(self, params)
    }
    fn eval(&mut self, inst: usize, inputs: &[(&str, Value)], outputs: &[(&str, &Type)]) -> Result<Vec<Value>, String> {
        crate::lua::LuaModule::eval(self, inst, inputs, outputs)
    }
    fn tick(
        &mut self,
        inst: usize,
        input: Option<&str>,
        inputs: &[(&str, Value)],
        outputs: &[(&str, &Type)],
    ) -> Result<Ticked, String> {
        crate::lua::LuaModule::tick(self, inst, input, inputs, outputs)
    }
}

/// An extension failed in a compiled program: says why and ends it.
pub fn fail(e: &str) -> ! {
    crate::io::flush();
    eprintln!("lineit: {e}");
    std::process::exit(1)
}
