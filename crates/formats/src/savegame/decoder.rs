//! The reader of the game-state stream: pointers, blocks, the map and the schema interpreter.

use super::*;

pub(super) struct Decoder<'a> {
    pub(super) r: Reader<'a>,
    pub(super) objects: BTreeMap<ObjId, Object>,
    pub(super) defining: BTreeSet<ObjId>,
}

pub(super) fn err<T>(at: usize, msg: impl Into<String>) -> Result<T> {
    Err(Error::Format(format!("{} at {at:#x}", msg.into())))
}

impl Decoder<'_> {
    pub(super) fn ptr(&mut self) -> Result<Option<ObjId>> {
        if !self.r.bool()? {
            return Ok(None);
        }
        let at = self.r.pos();
        let index = self.r.i32()?;
        let tag = self.r.u32()?;
        let ty = self.r.u32()?;
        if self.objects.contains_key(&index) || self.defining.contains(&index) {
            return Ok(Some(index));
        }
        let Some((name, ops)) = schema_for(ty) else {
            return Err(Error::Unsupported(format!(
                "object type {ty:#010x} (index {index}) at {at:#x}"
            )));
        };
        self.defining.insert(index);
        let fields = self.ops(ops, 0).map_err(|e| match e {
            Error::Format(m) => Error::Format(format!("{m} [in {name} at {at:#x}]")),
            Error::Truncated { at: t, wanted } => Error::Format(format!(
                "truncated: wanted {wanted} bytes at {t:#x} [in {name} at {at:#x}]"
            )),
            e => e,
        })?;
        self.defining.remove(&index);
        self.objects.insert(
            index,
            Object {
                index,
                tag,
                type_id: ty,
                type_name: name,
                fields,
            },
        );
        Ok(Some(index))
    }

    pub(super) fn ptrs(&mut self) -> Result<Vec<Option<ObjId>>> {
        let n = self.r.count()?;
        (0..n).map(|_| self.ptr()).collect()
    }

    /// A block read by `f`, which must consume it.
    pub(super) fn block<T>(&mut self, f: impl FnOnce(&mut Self, u32) -> Result<T>) -> Result<T> {
        let start = self.r.pos();
        if self.r.u32()? != BLOCK_START {
            return err(start, "expected a block");
        }
        let size = self.r.u32()? as usize;
        let version = self.r.u32()?;
        let end = self.r.pos() + size;
        if size < 4 || self.r.u32_at(end - 4) != Some(BLOCK_END) {
            return err(start, "block lacks its terminator");
        }
        let outer = self.r.limit(end - 4)?;
        let result = f(self, version);
        let unread = self.r.remaining();
        self.r.unlimit(outer);
        let value = result?;
        if unread != 0 {
            return err(
                start,
                format!("block (version {version}) has {unread} unread bytes"),
            );
        }
        self.r.set_pos(end);
        Ok(value)
    }

    /// `FUN_1400a45d0` with the cells of `FUN_14001b570`.
    pub(super) fn game_map(&mut self) -> Result<GameMap> {
        self.block(|d, version| {
            let size = d.r.i32()?;
            let n = d.r.count()?;
            let mut cells = Vec::with_capacity(n);
            for _ in 0..n {
                let entities = [d.ptr()?, d.ptr()?];
                let description = d.r.u32()?;
                let flags = d.r.bytes(9)?.try_into().unwrap();
                let value = d.r.u16()?;
                // Before version 3, another u16 (`FUN_14001b570`), not kept.
                if version < 3 {
                    d.r.u16()?;
                }
                let bytes = d.r.bytes(6)?.try_into().unwrap();
                cells.push(GameMapCell {
                    entities,
                    description,
                    flags,
                    value,
                    bytes,
                });
            }
            let mut trailer = [0; 9];
            for t in &mut trailer {
                *t = d.r.i16()?;
            }
            Ok(GameMap {
                size,
                cells,
                trailer,
            })
        })
    }

    /// One whole block's bytes, markers included.
    pub(super) fn raw_block(&mut self) -> Result<Vec<u8>> {
        let start = self.r.pos();
        self.skip_block()?;
        Ok(self.r.slice(start, self.r.pos())?.to_vec())
    }

    pub(super) fn skip_block(&mut self) -> Result<()> {
        self.block(|d, _| {
            d.r.skip_rest();
            Ok(())
        })
    }

    pub(super) fn ops(&mut self, ops: &[Op], version: u32) -> Result<Fields> {
        let mut out = Fields::default();
        self.ops_into(ops, version, &mut out)?;
        Ok(out)
    }

    pub(super) fn ops_into(&mut self, ops: &[Op], version: u32, out: &mut Fields) -> Result<()> {
        for op in ops {
            let (name, value) = match *op {
                // A byte other than 0 or 1 (an uninitialised bool) is kept as written.
                Op::Bool(n) => match self.r.u8()? {
                    b @ (0 | 1) => (n, Value::Bool(b == 1)),
                    b => (n, Value::I8(b as i8)),
                },
                Op::I8(n) => (n, Value::I8(self.r.u8()? as i8)),
                Op::I16(n) => (n, Value::I16(self.r.i16()?)),
                Op::U16(n) => (n, Value::U16(self.r.u16()?)),
                Op::I32(n) => (n, Value::I32(self.r.i32()?)),
                Op::U32(n) => (n, Value::U32(self.r.u32()?)),
                Op::Time(n) => (n, Value::I64(self.r.i64()?)),
                Op::F32(n) => (n, Value::F32(self.r.f32()?)),
                Op::Times(n, k) => (
                    n,
                    Value::I64s((0..k).map(|_| self.r.i64()).collect::<Result<_>>()?),
                ),
                Op::F32s(n, k) => (
                    n,
                    Value::F32s((0..k).map(|_| self.r.f32()).collect::<Result<_>>()?),
                ),
                Op::U32s(n, k) => (
                    n,
                    Value::U32s((0..k).map(|_| self.r.u32()).collect::<Result<_>>()?),
                ),
                Op::I32sBy(n, by) => {
                    let k = out.i32(by).unwrap_or(0).max(0) as usize;
                    (
                        n,
                        Value::U32s((0..k).map(|_| self.r.u32()).collect::<Result<_>>()?),
                    )
                }
                Op::F32sBy(n, by) => {
                    let k = out.i32(by).unwrap_or(0).max(0) as usize;
                    (
                        n,
                        Value::F32s((0..k).map(|_| self.r.f32()).collect::<Result<_>>()?),
                    )
                }
                Op::Str(n) => (n, Value::Str(self.r.string()?)),
                Op::WStr(n) => (n, Value::Str(self.r.wstring()?)),
                Op::Ext(n) => (n, Value::Ext(self.r.u32()?)),
                Op::Ptr(n) => (n, Value::Ptr(self.ptr()?)),
                Op::List(n, item) => {
                    let count = self.r.count()?;
                    let mut items = Vec::with_capacity(count.min(1 << 16));
                    for _ in 0..count {
                        items.push(self.ops(item, version)?);
                    }
                    (n, Value::List(items))
                }
                Op::Rest(n, item) => {
                    let mut items = Vec::new();
                    while self.r.remaining() > 0 {
                        items.push(self.ops(item, version)?);
                    }
                    (n, Value::List(items))
                }
                Op::Block(n, _, inner) => {
                    let (v, f) = self.block(|d, v| Ok((v, d.ops(inner, v)?)))?;
                    (n, Value::Block(v, f))
                }
                Op::Sub(inner) => {
                    self.ops_into(inner, version, out)?;
                    continue;
                }
                Op::If(cond, inner) => {
                    let take = match cond {
                        Cond::VersionAtLeast(v) => version >= v,
                        Cond::VersionBelow(v) => version < v,
                        Cond::Flag(f) => truthy(out.get(f)),
                    };
                    if take {
                        self.ops_into(inner, version, out)?;
                    }
                    continue;
                }
            };
            out.0.push((name, value));
        }
        Ok(())
    }
}
