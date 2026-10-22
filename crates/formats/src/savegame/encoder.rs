//! The writer of the game-state stream: pointers, blocks, the map and the schema interpreter.

use super::*;

pub(super) struct Encoder<'a> {
    pub(super) w: Writer,
    pub(super) objects: &'a BTreeMap<ObjId, Object>,
    pub(super) written: BTreeSet<ObjId>,
}

impl Encoder<'_> {
    pub(super) fn ptr(&mut self, p: Option<ObjId>) -> Result<()> {
        let Some(index) = p else {
            self.w.bool(false);
            return Ok(());
        };
        let o = self
            .objects
            .get(&index)
            .ok_or_else(|| Error::Format(format!("object {index} is not defined")))?;
        self.w.bool(true);
        self.w.i32(index);
        self.w.u32(o.tag);
        self.w.u32(o.type_id);
        if self.written.insert(index) {
            let (_, ops) = schema_for(o.type_id)
                .ok_or_else(|| Error::Unsupported(format!("object type {:#010x}", o.type_id)))?;
            self.ops(ops, 0, &o.fields)?;
        }
        Ok(())
    }

    pub(super) fn ptrs(&mut self, ptrs: &[Option<ObjId>]) -> Result<()> {
        self.w.count(ptrs.len());
        ptrs.iter().try_for_each(|&p| self.ptr(p))
    }

    pub(super) fn game_map(&mut self, map: &GameMap) -> Result<()> {
        let at = self.w.begin_block(4);
        self.w.i32(map.size);
        self.w.count(map.cells.len());
        for c in &map.cells {
            self.ptr(c.entities[0])?;
            self.ptr(c.entities[1])?;
            self.w.u32(c.description);
            self.w.bytes(&c.flags);
            self.w.u16(c.value);
            self.w.bytes(&c.bytes);
        }
        map.trailer.iter().for_each(|&t| self.w.i16(t));
        self.w.end_block(at);
        Ok(())
    }

    pub(super) fn ops(&mut self, ops: &[Op], version: u32, fields: &Fields) -> Result<()> {
        let mut next = fields.0.iter();
        self.ops_from(ops, version, fields, &mut next)?;
        match next.next() {
            None => Ok(()),
            Some((n, _)) => format_err(format!("field {n} has no step to write it")),
        }
    }

    /// Write `ops` from the fields `next` yields (`fields` is the whole set, for conditions).
    pub(super) fn ops_from<'f>(
        &mut self,
        ops: &[Op],
        version: u32,
        fields: &'f Fields,
        next: &mut std::slice::Iter<'f, (&'static str, Value)>,
    ) -> Result<()> {
        for op in ops {
            let name = match *op {
                Op::Sub(inner) => {
                    self.ops_from(inner, version, fields, next)?;
                    continue;
                }
                Op::If(cond, inner) => {
                    let take = match cond {
                        Cond::VersionAtLeast(v) => version >= v,
                        Cond::VersionBelow(v) => version < v,
                        Cond::Flag(f) => truthy(fields.get(f)),
                    };
                    if take {
                        self.ops_from(inner, version, fields, next)?;
                    }
                    continue;
                }
                Op::Bool(n) | Op::I8(n) | Op::I16(n) | Op::U16(n) | Op::I32(n) | Op::U32(n) => n,
                Op::Time(n) | Op::F32(n) | Op::Times(n, _) | Op::F32s(n, _) | Op::U32s(n, _) => n,
                Op::I32sBy(n, _) | Op::F32sBy(n, _) | Op::Str(n) | Op::WStr(n) | Op::Ext(n) => n,
                Op::Ptr(n) | Op::List(n, _) | Op::Rest(n, _) | Op::Block(n, _, _) => n,
            };
            let Some((field, value)) = next.next() else {
                return format_err(format!("missing field {name}"));
            };
            if *field != name {
                return format_err(format!("expected field {name}, found {field}"));
            }
            let w = &mut self.w;
            match (*op, value) {
                (Op::Bool(_), Value::Bool(v)) => w.bool(*v),
                (Op::Bool(_), Value::I8(v)) => w.u8(*v as u8),
                (Op::I8(_), Value::I8(v)) => w.u8(*v as u8),
                (Op::I16(_), Value::I16(v)) => w.i16(*v),
                (Op::U16(_), Value::U16(v)) => w.u16(*v),
                (Op::I32(_), Value::I32(v)) => w.i32(*v),
                (Op::U32(_), Value::U32(v)) | (Op::Ext(_), Value::Ext(v)) => w.u32(*v),
                (Op::Time(_), Value::I64(v)) => w.i64(*v),
                (Op::F32(_), Value::F32(v)) => w.f32(*v),
                (Op::Times(_, _), Value::I64s(v)) => v.iter().for_each(|&t| w.i64(t)),
                (Op::F32s(_, _) | Op::F32sBy(_, _), Value::F32s(v)) => {
                    v.iter().for_each(|&f| w.f32(f))
                }
                (Op::U32s(_, _) | Op::I32sBy(_, _), Value::U32s(v)) => {
                    v.iter().for_each(|&u| w.u32(u))
                }
                (Op::Str(_), Value::Str(v)) => w.string(v),
                (Op::WStr(_), Value::Str(v)) => w.wstring(v),
                (Op::Ptr(_), Value::Ptr(p)) => self.ptr(*p)?,
                (Op::List(_, item), Value::List(items)) => {
                    w.count(items.len());
                    for f in items {
                        self.ops(item, version, f)?;
                    }
                }
                (Op::Rest(_, item), Value::List(items)) => {
                    for f in items {
                        self.ops(item, version, f)?;
                    }
                }
                (Op::Block(_, _, inner), Value::Block(v, f)) => {
                    let at = w.begin_block(*v);
                    self.ops(inner, *v, f)?;
                    self.w.end_block(at);
                }
                (op, value) => return format_err(format!("{op:?} cannot write {value:?}")),
            }
        }
        Ok(())
    }
}
