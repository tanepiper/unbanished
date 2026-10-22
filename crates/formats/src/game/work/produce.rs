//! Consume-produce descriptions: the recipes a workplace turns inputs into outputs with.

use super::*;

/// One production recipe of a [`ConsumeProduceDescription`]: what a workplace uses and what it
/// makes.
///
/// Original: the 0x28-byte record read by `FUN_140033050`.
#[derive(Debug, Clone, Default)]
pub struct ProduceRecipe {
    // +0x00/+0x08
    /// Input raw-material templates and counts.
    pub inputs: Vec<(Option<String>, i32)>,
    // +0x10
    /// The output raw-material template.
    pub output: Option<String>,
    // +0x18; +0x170
    /// Stock group (manager slot of the limit check).
    pub group: i32,
    // +0x20
    /// The UI label key.
    pub label: String,
}

/// `ConsumeProduceDescription` (`Template\X.rsc:consumeproduce`).
///
/// Original: `FUN_14011e5b0`.
#[derive(Debug, Clone, Default)]
pub struct ConsumeProduceDescription {
    // +0x20
    /// String table of the recipe labels.
    pub string_table: Option<String>,
    // +0x28/+0x30
    /// The recipes.
    pub recipes: Vec<ProduceRecipe>,
    // +0x40
    /// Work cycles per production.
    pub cycles: i32,
    // +0x50
    /// Work type (tool mesh, PerformWork animation `type + 9` below 11, walk mode).
    pub work_type: u32,
    // +0x38 / +0x3c
    /// The type of a message sent to the building's component of that type after each
    /// production.
    pub message_type: u32,
    /// The value of that message.
    pub message_value: i32,
    // +0x44
    /// Seconds per work cycle.
    pub duration: f32,
    // +0x48
    /// Productions before the workplace is exhausted (−1: never).
    pub lifetime: i32,
    // +0x4c
    /// Initial recipe selector (−1: weighted random).
    pub selector: i32,
}

/// Read a [`ConsumeProduceDescription`] from the bytes of its resource.
pub fn consume_produce_description(data: &[u8]) -> Result<ConsumeProduceDescription> {
    let (_, value) = crs::parse(data, |r: &mut Reader, h| {
        if h.type_id != type_id("ConsumeProduceDescription") {
            return format_err(format!(
                "expected ConsumeProduceDescription, found {:#010x}",
                h.type_id
            ));
        }
        r.block(|r, _| {
            let string_table = r.external_ref()?;
            let n = r.count()?;
            let mut recipes = Vec::with_capacity(n);
            for _ in 0..n {
                let k = r.count()?;
                let inputs = (0..k)
                    .map(|_| Ok((r.external_ref()?, r.i32()?)))
                    .collect::<Result<_>>()?;
                recipes.push(ProduceRecipe {
                    inputs,
                    output: r.external_ref()?,
                    group: r.i32()?,
                    label: r.string()?,
                });
            }
            Ok(ConsumeProduceDescription {
                string_table,
                recipes,
                cycles: r.i32()?,
                work_type: r.u32()?,
                message_type: r.u32()?,
                message_value: r.i32()?,
                duration: r.f32()?,
                lifetime: r.i32()?,
                selector: r.i32()?,
            })
        })
    })?;
    Ok(value)
}
