//! Code for dealing with [LLVM][llvm-intrinsics] and VM intrinsics.
//!
//! VM intrinsics are used to interact with the host VM.
//!
//! [llvm-intrinsics]: https://llvm.org/docs/LangRef.html#intrinsic-functions

use crate::abi::Abi;
use inkwell::{
    attributes::{Attribute, AttributeLoc},
    builder::Builder,
    context::Context,
    module::{Linkage, Module},
    types::{
        BasicType, BasicTypeEnum, FloatType, IntType, PointerType, StructType, VectorType, VoidType,
    },
    values::{
        BasicValue, BasicValueEnum, FloatValue, FunctionValue, InstructionValue, IntValue,
        PointerValue, VectorValue,
    },
    AddressSpace,
};
use std::collections::{hash_map::Entry, HashMap};
use wasmer_compiler::CompileError;
use wasmer_types::entity::{EntityRef, PrimaryMap};
use wasmer_types::{
    FunctionIndex, FunctionType as FuncType, GlobalIndex, LocalFunctionIndex, MemoryIndex,
    Mutability, SignatureIndex, TableIndex, Type,
};
use wasmer_vm::ModuleInfo as WasmerCompilerModule;
use wasmer_vm::{MemoryStyle, TrapCode, VMBuiltinFunctionIndex, VMOffsets};

pub fn type_to_llvm_ptr<'ctx>(
    intrinsics: &Intrinsics<'ctx>,
    ty: Type,
) -> Result<PointerType<'ctx>, CompileError> {
    match ty {
        Type::I32 => Ok(intrinsics.i32_ptr_ty),
        Type::I64 => Ok(intrinsics.i64_ptr_ty),
        Type::F32 => Ok(intrinsics.f32_ptr_ty),
        Type::F64 => Ok(intrinsics.f64_ptr_ty),
        Type::V128 => Ok(intrinsics.i128_ptr_ty),
        Type::FuncRef => Ok(intrinsics.funcref_ty.ptr_type(AddressSpace::Generic)),
        Type::ExternRef => Ok(intrinsics.externref_ty.ptr_type(AddressSpace::Generic)),
    }
}

pub fn type_to_llvm<'ctx>(
    intrinsics: &Intrinsics<'ctx>,
    ty: Type,
) -> Result<BasicTypeEnum<'ctx>, CompileError> {
    match ty {
        Type::I32 => Ok(intrinsics.i32_ty.as_basic_type_enum()),
        Type::I64 => Ok(intrinsics.i64_ty.as_basic_type_enum()),
        Type::F32 => Ok(intrinsics.f32_ty.as_basic_type_enum()),
        Type::F64 => Ok(intrinsics.f64_ty.as_basic_type_enum()),
        Type::V128 => Ok(intrinsics.i128_ty.as_basic_type_enum()),
        Type::FuncRef => Ok(intrinsics.funcref_ty.as_basic_type_enum()),
        Type::ExternRef => Ok(intrinsics.externref_ty.as_basic_type_enum()),
    }
}

/// Struct containing LLVM and VM intrinsics.
pub struct Intrinsics<'ctx> {
    pub ctlz_i32: FunctionValue<'ctx>,
    pub ctlz_i64: FunctionValue<'ctx>,

    pub cttz_i32: FunctionValue<'ctx>,
    pub cttz_i64: FunctionValue<'ctx>,

    pub ctpop_i32: FunctionValue<'ctx>,
    pub ctpop_i64: FunctionValue<'ctx>,
    pub ctpop_i8x16: FunctionValue<'ctx>,

    pub sqrt_f32: FunctionValue<'ctx>,
    pub sqrt_f64: FunctionValue<'ctx>,
    pub sqrt_f32x4: FunctionValue<'ctx>,
    pub sqrt_f64x2: FunctionValue<'ctx>,

    pub ceil_f32: FunctionValue<'ctx>,
    pub ceil_f64: FunctionValue<'ctx>,
    pub ceil_f32x4: FunctionValue<'ctx>,
    pub ceil_f64x2: FunctionValue<'ctx>,

    pub floor_f32: FunctionValue<'ctx>,
    pub floor_f64: FunctionValue<'ctx>,
    pub floor_f32x4: FunctionValue<'ctx>,
    pub floor_f64x2: FunctionValue<'ctx>,

    pub trunc_f32: FunctionValue<'ctx>,
    pub trunc_f64: FunctionValue<'ctx>,
    pub trunc_f32x4: FunctionValue<'ctx>,
    pub trunc_f64x2: FunctionValue<'ctx>,

    pub nearbyint_f32: FunctionValue<'ctx>,
    pub nearbyint_f64: FunctionValue<'ctx>,
    pub nearbyint_f32x4: FunctionValue<'ctx>,
    pub nearbyint_f64x2: FunctionValue<'ctx>,

    pub fabs_f32: FunctionValue<'ctx>,
    pub fabs_f64: FunctionValue<'ctx>,
    pub fabs_f32x4: FunctionValue<'ctx>,
    pub fabs_f64x2: FunctionValue<'ctx>,

    pub copysign_f32: FunctionValue<'ctx>,
    pub copysign_f64: FunctionValue<'ctx>,
    pub copysign_f32x4: FunctionValue<'ctx>,
    pub copysign_f64x2: FunctionValue<'ctx>,

    pub sadd_sat_i8x16: FunctionValue<'ctx>,
    pub sadd_sat_i16x8: FunctionValue<'ctx>,
    pub uadd_sat_i8x16: FunctionValue<'ctx>,
    pub uadd_sat_i16x8: FunctionValue<'ctx>,

    pub ssub_sat_i8x16: FunctionValue<'ctx>,
    pub ssub_sat_i16x8: FunctionValue<'ctx>,
    pub usub_sat_i8x16: FunctionValue<'ctx>,
    pub usub_sat_i16x8: FunctionValue<'ctx>,

    pub expect_i1: FunctionValue<'ctx>,
    pub trap: FunctionValue<'ctx>,
    pub debug_trap: FunctionValue<'ctx>,

    pub personality: FunctionValue<'ctx>,
    pub readonly: Attribute,
    pub stack_probe: Attribute,

    pub void_ty: VoidType<'ctx>,
    pub i1_ty: IntType<'ctx>,
    pub i2_ty: IntType<'ctx>,
    pub i4_ty: IntType<'ctx>,
    pub i8_ty: IntType<'ctx>,
    pub i16_ty: IntType<'ctx>,
    pub i32_ty: IntType<'ctx>,
    pub i64_ty: IntType<'ctx>,
    pub i128_ty: IntType<'ctx>,
    pub f32_ty: FloatType<'ctx>,
    pub f64_ty: FloatType<'ctx>,

    pub i1x128_ty: VectorType<'ctx>,
    pub i8x16_ty: VectorType<'ctx>,
    pub i16x8_ty: VectorType<'ctx>,
    pub i32x4_ty: VectorType<'ctx>,
    pub i64x2_ty: VectorType<'ctx>,
    pub f32x4_ty: VectorType<'ctx>,
    pub f64x2_ty: VectorType<'ctx>,
    pub i32x8_ty: VectorType<'ctx>,

    pub i8_ptr_ty: PointerType<'ctx>,
    pub i16_ptr_ty: PointerType<'ctx>,
    pub i32_ptr_ty: PointerType<'ctx>,
    pub i64_ptr_ty: PointerType<'ctx>,
    pub i128_ptr_ty: PointerType<'ctx>,
    pub f32_ptr_ty: PointerType<'ctx>,
    pub f64_ptr_ty: PointerType<'ctx>,

    pub anyfunc_ty: StructType<'ctx>,

    pub funcref_ty: PointerType<'ctx>,
    pub externref_ty: PointerType<'ctx>,
    pub anyref_ty: PointerType<'ctx>,

    pub i1_zero: IntValue<'ctx>,
    pub i8_zero: IntValue<'ctx>,
    pub i32_zero: IntValue<'ctx>,
    pub i64_zero: IntValue<'ctx>,
    pub i128_zero: IntValue<'ctx>,
    pub f32_zero: FloatValue<'ctx>,
    pub f64_zero: FloatValue<'ctx>,
    pub f32x4_zero: VectorValue<'ctx>,
    pub f64x2_zero: VectorValue<'ctx>,
    pub i32_consts: [IntValue<'ctx>; 16],

    pub trap_unreachable: BasicValueEnum<'ctx>,
    pub trap_call_indirect_null: BasicValueEnum<'ctx>,
    pub trap_call_indirect_sig: BasicValueEnum<'ctx>,
    pub trap_memory_oob: BasicValueEnum<'ctx>,
    pub trap_illegal_arithmetic: BasicValueEnum<'ctx>,
    pub trap_integer_division_by_zero: BasicValueEnum<'ctx>,
    pub trap_bad_conversion_to_integer: BasicValueEnum<'ctx>,
    pub trap_unaligned_atomic: BasicValueEnum<'ctx>,
    pub trap_table_access_oob: BasicValueEnum<'ctx>,

    pub experimental_stackmap: FunctionValue<'ctx>,

    // VM libcalls.
    pub table_copy: FunctionValue<'ctx>,
    pub table_init: FunctionValue<'ctx>,
    pub table_fill: FunctionValue<'ctx>,
    pub table_size: FunctionValue<'ctx>,
    pub imported_table_size: FunctionValue<'ctx>,
    pub table_get: FunctionValue<'ctx>,
    pub imported_table_get: FunctionValue<'ctx>,
    pub table_set: FunctionValue<'ctx>,
    pub imported_table_set: FunctionValue<'ctx>,
    pub table_grow: FunctionValue<'ctx>,
    pub imported_table_grow: FunctionValue<'ctx>,
    pub memory_init: FunctionValue<'ctx>,
    pub data_drop: FunctionValue<'ctx>,
    pub func_ref: FunctionValue<'ctx>,
    pub elem_drop: FunctionValue<'ctx>,
    pub memory_copy: FunctionValue<'ctx>,
    pub imported_memory_copy: FunctionValue<'ctx>,
    pub memory_fill: FunctionValue<'ctx>,
    pub imported_memory_fill: FunctionValue<'ctx>,

    pub throw_trap: FunctionValue<'ctx>,

    // VM builtins.
    pub vmfunction_import_ptr_ty: PointerType<'ctx>,
    pub vmfunction_import_body_element: u32,
    pub vmfunction_import_vmctx_element: u32,

    pub vmmemory_definition_ptr_ty: PointerType<'ctx>,
    pub vmmemory_definition_base_element: u32,
    pub vmmemory_definition_current_length_element: u32,

    pub memory32_grow_ptr_ty: PointerType<'ctx>,
    pub imported_memory32_grow_ptr_ty: PointerType<'ctx>,
    pub memory32_size_ptr_ty: PointerType<'ctx>,
    pub imported_memory32_size_ptr_ty: PointerType<'ctx>,

    // Pointer to the VM.
    pub ctx_ptr_ty: PointerType<'ctx>,
}

impl<'ctx> Intrinsics<'ctx> {
    /// Create an [`Intrinsics`] for the given [`Context`].
    pub fn declare(module: &Module<'ctx>, context: &'ctx Context) -> Self {
        let void_ty = context.void_type();
        let i1_ty = context.bool_type();
        let i2_ty = context.custom_width_int_type(2);
        let i4_ty = context.custom_width_int_type(4);
        let i8_ty = context.i8_type();
        let i16_ty = context.i16_type();
        let i32_ty = context.i32_type();
        let i64_ty = context.i64_type();
        let i128_ty = context.i128_type();
        let f32_ty = context.f32_type();
        let f64_ty = context.f64_type();

        let i1x128_ty = i1_ty.vec_type(128);
        let i8x16_ty = i8_ty.vec_type(16);
        let i16x8_ty = i16_ty.vec_type(8);
        let i32x4_ty = i32_ty.vec_type(4);
        let i64x2_ty = i64_ty.vec_type(2);
        let f32x4_ty = f32_ty.vec_type(4);
        let f64x2_ty = f64_ty.vec_type(2);
        let i32x8_ty = i32_ty.vec_type(8);

        let i8_ptr_ty = i8_ty.ptr_type(AddressSpace::Generic);
        let i16_ptr_ty = i16_ty.ptr_type(AddressSpace::Generic);
        let i32_ptr_ty = i32_ty.ptr_type(AddressSpace::Generic);
        let i64_ptr_ty = i64_ty.ptr_type(AddressSpace::Generic);
        let i128_ptr_ty = i128_ty.ptr_type(AddressSpace::Generic);
        let f32_ptr_ty = f32_ty.ptr_type(AddressSpace::Generic);
        let f64_ptr_ty = f64_ty.ptr_type(AddressSpace::Generic);

        let i1_zero = i1_ty.const_int(0, false);
        let i8_zero = i8_ty.const_int(0, false);
        let i32_zero = i32_ty.const_int(0, false);
        let i64_zero = i64_ty.const_int(0, false);
        let i128_zero = i128_ty.const_int(0, false);
        let f32_zero = f32_ty.const_float(0.0);
        let f64_zero = f64_ty.const_float(0.0);
        let f32x4_zero = f32x4_ty.const_zero();
        let f64x2_zero = f64x2_ty.const_zero();
        let i32_consts = [
            i32_ty.const_int(0, false),
            i32_ty.const_int(1, false),
            i32_ty.const_int(2, false),
            i32_ty.const_int(3, false),
            i32_ty.const_int(4, false),
            i32_ty.const_int(5, false),
            i32_ty.const_int(6, false),
            i32_ty.const_int(7, false),
            i32_ty.const_int(8, false),
            i32_ty.const_int(9, false),
            i32_ty.const_int(10, false),
            i32_ty.const_int(11, false),
            i32_ty.const_int(12, false),
            i32_ty.const_int(13, false),
            i32_ty.const_int(14, false),
            i32_ty.const_int(15, false),
        ];

        let i1_ty_basic = i1_ty.as_basic_type_enum();
        let i32_ty_basic = i32_ty.as_basic_type_enum();
        let i64_ty_basic = i64_ty.as_basic_type_enum();
        let f32_ty_basic = f32_ty.as_basic_type_enum();
        let f64_ty_basic = f64_ty.as_basic_type_enum();
        let i8x16_ty_basic = i8x16_ty.as_basic_type_enum();
        let i16x8_ty_basic = i16x8_ty.as_basic_type_enum();
        let f32x4_ty_basic = f32x4_ty.as_basic_type_enum();
        let f64x2_ty_basic = f64x2_ty.as_basic_type_enum();
        let i8_ptr_ty_basic = i8_ptr_ty.as_basic_type_enum();

        let ctx_ty = i8_ty;
        let ctx_ptr_ty = ctx_ty.ptr_type(AddressSpace::Generic);

        let sigindex_ty = i32_ty;

        let anyfunc_ty = context.struct_type(
            &[
                i8_ptr_ty_basic,
                sigindex_ty.as_basic_type_enum(),
                ctx_ptr_ty.as_basic_type_enum(),
            ],
            false,
        );
        let funcref_ty = anyfunc_ty.ptr_type(AddressSpace::Generic);
        let externref_ty = funcref_ty;
        let anyref_ty = i8_ptr_ty;

        let ret_i8x16_take_i8x16 = i8x16_ty.fn_type(&[i8x16_ty_basic], false);
        let ret_i8x16_take_i8x16_i8x16 = i8x16_ty.fn_type(&[i8x16_ty_basic, i8x16_ty_basic], false);
        let ret_i16x8_take_i16x8_i16x8 = i16x8_ty.fn_type(&[i16x8_ty_basic, i16x8_ty_basic], false);

        let ret_i32_take_i32_i1 = i32_ty.fn_type(&[i32_ty_basic, i1_ty_basic], false);
        let ret_i64_take_i64_i1 = i64_ty.fn_type(&[i64_ty_basic, i1_ty_basic], false);

        let ret_i32_take_i32 = i32_ty.fn_type(&[i32_ty_basic], false);
        let ret_i64_take_i64 = i64_ty.fn_type(&[i64_ty_basic], false);

        let ret_f32_take_f32 = f32_ty.fn_type(&[f32_ty_basic], false);
        let ret_f64_take_f64 = f64_ty.fn_type(&[f64_ty_basic], false);
        let ret_f32x4_take_f32x4 = f32x4_ty.fn_type(&[f32x4_ty_basic], false);
        let ret_f64x2_take_f64x2 = f64x2_ty.fn_type(&[f64x2_ty_basic], false);

        let ret_f32_take_f32_f32 = f32_ty.fn_type(&[f32_ty_basic, f32_ty_basic], false);
        let ret_f64_take_f64_f64 = f64_ty.fn_type(&[f64_ty_basic, f64_ty_basic], false);
        let ret_f32x4_take_f32x4_f32x4 = f32x4_ty.fn_type(&[f32x4_ty_basic, f32x4_ty_basic], false);
        let ret_f64x2_take_f64x2_f64x2 = f64x2_ty.fn_type(&[f64x2_ty_basic, f64x2_ty_basic], false);

        let ret_i1_take_i1_i1 = i1_ty.fn_type(&[i1_ty_basic, i1_ty_basic], false);
        let intrinsics = Self {
            ctlz_i32: module.add_function("llvm.ctlz.i32", ret_i32_take_i32_i1, None),
            ctlz_i64: module.add_function("llvm.ctlz.i64", ret_i64_take_i64_i1, None),

            cttz_i32: module.add_function("llvm.cttz.i32", ret_i32_take_i32_i1, None),
            cttz_i64: module.add_function("llvm.cttz.i64", ret_i64_take_i64_i1, None),

            ctpop_i32: module.add_function("llvm.ctpop.i32", ret_i32_take_i32, None),
            ctpop_i64: module.add_function("llvm.ctpop.i64", ret_i64_take_i64, None),
            ctpop_i8x16: module.add_function("llvm.ctpop.v16i8", ret_i8x16_take_i8x16, None),

            sqrt_f32: module.add_function("llvm.sqrt.f32", ret_f32_take_f32, None),
            sqrt_f64: module.add_function("llvm.sqrt.f64", ret_f64_take_f64, None),
            sqrt_f32x4: module.add_function("llvm.sqrt.v4f32", ret_f32x4_take_f32x4, None),
            sqrt_f64x2: module.add_function("llvm.sqrt.v2f64", ret_f64x2_take_f64x2, None),

            ceil_f32: module.add_function("llvm.ceil.f32", ret_f32_take_f32, None),
            ceil_f64: module.add_function("llvm.ceil.f64", ret_f64_take_f64, None),
            ceil_f32x4: module.add_function("llvm.ceil.v4f32", ret_f32x4_take_f32x4, None),
            ceil_f64x2: module.add_function("llvm.ceil.v2f64", ret_f64x2_take_f64x2, None),

            floor_f32: module.add_function("llvm.floor.f32", ret_f32_take_f32, None),
            floor_f64: module.add_function("llvm.floor.f64", ret_f64_take_f64, None),
            floor_f32x4: module.add_function("llvm.floor.v4f32", ret_f32x4_take_f32x4, None),
            floor_f64x2: module.add_function("llvm.floor.v2f64", ret_f64x2_take_f64x2, None),

            trunc_f32: module.add_function("llvm.trunc.f32", ret_f32_take_f32, None),
            trunc_f64: module.add_function("llvm.trunc.f64", ret_f64_take_f64, None),
            trunc_f32x4: module.add_function("llvm.trunc.v4f32", ret_f32x4_take_f32x4, None),
            trunc_f64x2: module.add_function("llvm.trunc.v2f64", ret_f64x2_take_f64x2, None),

            nearbyint_f32: module.add_function("llvm.nearbyint.f32", ret_f32_take_f32, None),
            nearbyint_f64: module.add_function("llvm.nearbyint.f64", ret_f64_take_f64, None),
            nearbyint_f32x4: module.add_function(
                "llvm.nearbyint.v4f32",
                ret_f32x4_take_f32x4,
                None,
            ),
            nearbyint_f64x2: module.add_function(
                "llvm.nearbyint.v2f64",
                ret_f64x2_take_f64x2,
                None,
            ),

            fabs_f32: module.add_function("llvm.fabs.f32", ret_f32_take_f32, None),
            fabs_f64: module.add_function("llvm.fabs.f64", ret_f64_take_f64, None),
            fabs_f32x4: module.add_function("llvm.fabs.v4f32", ret_f32x4_take_f32x4, None),
            fabs_f64x2: module.add_function("llvm.fabs.v2f64", ret_f64x2_take_f64x2, None),

            copysign_f32: module.add_function("llvm.copysign.f32", ret_f32_take_f32_f32, None),
            copysign_f64: module.add_function("llvm.copysign.f64", ret_f64_take_f64_f64, None),
            copysign_f32x4: module.add_function(
                "llvm.copysign.v4f32",
                ret_f32x4_take_f32x4_f32x4,
                None,
            ),
            copysign_f64x2: module.add_function(
                "llvm.copysign.v2f64",
                ret_f64x2_take_f64x2_f64x2,
                None,
            ),

            sadd_sat_i8x16: module.add_function(
                "llvm.sadd.sat.v16i8",
                ret_i8x16_take_i8x16_i8x16,
                None,
            ),
            sadd_sat_i16x8: module.add_function(
                "llvm.sadd.sat.v8i16",
                ret_i16x8_take_i16x8_i16x8,
                None,
            ),
            uadd_sat_i8x16: module.add_function(
                "llvm.uadd.sat.v16i8",
                ret_i8x16_take_i8x16_i8x16,
                None,
            ),
            uadd_sat_i16x8: module.add_function(
                "llvm.uadd.sat.v8i16",
                ret_i16x8_take_i16x8_i16x8,
                None,
            ),

            ssub_sat_i8x16: module.add_function(
                "llvm.ssub.sat.v16i8",
                ret_i8x16_take_i8x16_i8x16,
                None,
            ),
            ssub_sat_i16x8: module.add_function(
                "llvm.ssub.sat.v8i16",
                ret_i16x8_take_i16x8_i16x8,
                None,
            ),
            usub_sat_i8x16: module.add_function(
                "llvm.usub.sat.v16i8",
                ret_i8x16_take_i8x16_i8x16,
                None,
            ),
            usub_sat_i16x8: module.add_function(
                "llvm.usub.sat.v8i16",
                ret_i16x8_take_i16x8_i16x8,
                None,
            ),

            expect_i1: module.add_function("llvm.expect.i1", ret_i1_take_i1_i1, None),
            trap: module.add_function("llvm.trap", void_ty.fn_type(&[], false), None),
            debug_trap: module.add_function("llvm.debugtrap", void_ty.fn_type(&[], false), None),
            personality: module.add_function(
                "__gxx_personality_v0",
                i32_ty.fn_type(&[], false),
                Some(Linkage::External),
            ),
            readonly: context
                .create_enum_attribute(Attribute::get_named_enum_kind_id("readonly"), 0),
            stack_probe: context.create_string_attribute("probe-stack", "wasmer_vm_probestack"),

            void_ty,
            i1_ty,
            i2_ty,
            i4_ty,
            i8_ty,
            i16_ty,
            i32_ty,
            i64_ty,
            i128_ty,
            f32_ty,
            f64_ty,

            i1x128_ty,
            i8x16_ty,
            i16x8_ty,
            i32x4_ty,
            i64x2_ty,
            f32x4_ty,
            f64x2_ty,
            i32x8_ty,

            i8_ptr_ty,
            i16_ptr_ty,
            i32_ptr_ty,
            i64_ptr_ty,
            i128_ptr_ty,
            f32_ptr_ty,
            f64_ptr_ty,

            anyfunc_ty,

            funcref_ty,
            externref_ty,
            anyref_ty,

            i1_zero,
            i8_zero,
            i32_zero,
            i64_zero,
            i128_zero,
            f32_zero,
            f64_zero,
            f32x4_zero,
            f64x2_zero,
            i32_consts,

            trap_unreachable: i32_ty
                .const_int(TrapCode::UnreachableCodeReached as _, false)
                .as_basic_value_enum(),
            trap_call_indirect_null: i32_ty
                .const_int(TrapCode::IndirectCallToNull as _, false)
                .as_basic_value_enum(),
            trap_call_indirect_sig: i32_ty
                .const_int(TrapCode::BadSignature as _, false)
                .as_basic_value_enum(),
            trap_memory_oob: i32_ty
                .const_int(TrapCode::HeapAccessOutOfBounds as _, false)
                .as_basic_value_enum(),
            trap_illegal_arithmetic: i32_ty
                .const_int(TrapCode::IntegerOverflow as _, false)
                .as_basic_value_enum(),
            trap_integer_division_by_zero: i32_ty
                .const_int(TrapCode::IntegerDivisionByZero as _, false)
                .as_basic_value_enum(),
            trap_bad_conversion_to_integer: i32_ty
                .const_int(TrapCode::BadConversionToInteger as _, false)
                .as_basic_value_enum(),
            trap_unaligned_atomic: i32_ty
                .const_int(TrapCode::UnalignedAtomic as _, false)
                .as_basic_value_enum(),
            trap_table_access_oob: i32_ty
                .const_int(TrapCode::TableAccessOutOfBounds as _, false)
                .as_basic_value_enum(),

            experimental_stackmap: module.add_function(
                "llvm.experimental.stackmap",
                void_ty.fn_type(
                    &[
                        i64_ty_basic, /* id */
                        i32_ty_basic, /* numShadowBytes */
                    ],
                    true,
                ),
                None,
            ),

            // VM libcalls.
            table_copy: module.add_function(
                "wasmer_vm_table_copy",
                void_ty.fn_type(
                    &[
                        ctx_ptr_ty.as_basic_type_enum(),
                        i32_ty_basic,
                        i32_ty_basic,
                        i32_ty_basic,
                        i32_ty_basic,
                        i32_ty_basic,
                    ],
                    false,
                ),
                None,
            ),
            table_init: module.add_function(
                "wasmer_vm_table_init",
                void_ty.fn_type(
                    &[
                        ctx_ptr_ty.as_basic_type_enum(),
                        i32_ty_basic,
                        i32_ty_basic,
                        i32_ty_basic,
                        i32_ty_basic,
                        i32_ty_basic,
                    ],
                    false,
                ),
                None,
            ),
            table_fill: module.add_function(
                "wasmer_vm_table_fill",
                void_ty.fn_type(
                    &[
                        ctx_ptr_ty.as_basic_type_enum(),
                        i32_ty_basic,
                        i32_ty_basic,
                        anyref_ty.as_basic_type_enum(),
                        i32_ty_basic,
                    ],
                    false,
                ),
                None,
            ),
            table_size: module.add_function(
                "wasmer_vm_table_size",
                i32_ty.fn_type(&[ctx_ptr_ty.as_basic_type_enum(), i32_ty_basic], false),
                None,
            ),
            imported_table_size: module.add_function(
                "wasmer_vm_imported_table_size",
                i32_ty.fn_type(&[ctx_ptr_ty.as_basic_type_enum(), i32_ty_basic], false),
                None,
            ),
            table_get: module.add_function(
                "wasmer_vm_table_get",
                anyref_ty.fn_type(
                    &[ctx_ptr_ty.as_basic_type_enum(), i32_ty_basic, i32_ty_basic],
                    false,
                ),
                None,
            ),
            imported_table_get: module.add_function(
                "wasmer_vm_imported_table_get",
                anyref_ty.fn_type(
                    &[ctx_ptr_ty.as_basic_type_enum(), i32_ty_basic, i32_ty_basic],
                    false,
                ),
                None,
            ),
            table_set: module.add_function(
                "wasmer_vm_table_set",
                void_ty.fn_type(
                    &[
                        ctx_ptr_ty.as_basic_type_enum(),
                        i32_ty_basic,
                        i32_ty_basic,
                        anyref_ty.as_basic_type_enum(),
                    ],
                    false,
                ),
                None,
            ),
            imported_table_set: module.add_function(
                "wasmer_vm_imported_table_set",
                void_ty.fn_type(
                    &[
                        ctx_ptr_ty.as_basic_type_enum(),
                        i32_ty_basic,
                        i32_ty_basic,
                        anyref_ty.as_basic_type_enum(),
                    ],
                    false,
                ),
                None,
            ),
            table_grow: module.add_function(
                "wasmer_vm_table_grow",
                i32_ty.fn_type(
                    &[
                        ctx_ptr_ty.as_basic_type_enum(),
                        anyref_ty.as_basic_type_enum(),
                        i32_ty_basic,
                        i32_ty_basic,
                    ],
                    false,
                ),
                None,
            ),
            imported_table_grow: module.add_function(
                "wasmer_vm_imported_table_grow",
                i32_ty.fn_type(
                    &[
                        ctx_ptr_ty.as_basic_type_enum(),
                        anyref_ty.as_basic_type_enum(),
                        i32_ty_basic,
                        i32_ty_basic,
                    ],
                    false,
                ),
                None,
            ),
            memory_init: module.add_function(
                "wasmer_vm_memory32_init",
                void_ty.fn_type(
                    &[
                        ctx_ptr_ty.as_basic_type_enum(),
                        i32_ty_basic,
                        i32_ty_basic,
                        i32_ty_basic,
                        i32_ty_basic,
                        i32_ty_basic,
                    ],
                    false,
                ),
                None,
            ),
            memory_copy: module.add_function(
                "wasmer_vm_memory32_copy",
                void_ty.fn_type(
                    &[
                        ctx_ptr_ty.as_basic_type_enum(),
                        i32_ty_basic,
                        i32_ty_basic,
                        i32_ty_basic,
                        i32_ty_basic,
                    ],
                    false,
                ),
                None,
            ),
            imported_memory_copy: module.add_function(
                "wasmer_vm_imported_memory32_copy",
                void_ty.fn_type(
                    &[
                        ctx_ptr_ty.as_basic_type_enum(),
                        i32_ty_basic,
                        i32_ty_basic,
                        i32_ty_basic,
                        i32_ty_basic,
                    ],
                    false,
                ),
                None,
            ),
            memory_fill: module.add_function(
                "wasmer_vm_memory32_fill",
                void_ty.fn_type(
                    &[
                        ctx_ptr_ty.as_basic_type_enum(),
                        i32_ty_basic,
                        i32_ty_basic,
                        i32_ty_basic,
                        i32_ty_basic,
                    ],
                    false,
                ),
                None,
            ),
            imported_memory_fill: module.add_function(
                "wasmer_vm_imported_memory32_fill",
                void_ty.fn_type(
                    &[
                        ctx_ptr_ty.as_basic_type_enum(),
                        i32_ty_basic,
                        i32_ty_basic,
                        i32_ty_basic,
                        i32_ty_basic,
                    ],
                    false,
                ),
                None,
            ),
            data_drop: module.add_function(
                "wasmer_vm_data_drop",
                void_ty.fn_type(&[ctx_ptr_ty.as_basic_type_enum(), i32_ty_basic], false),
                None,
            ),
            func_ref: module.add_function(
                "wasmer_vm_func_ref",
                funcref_ty.fn_type(&[ctx_ptr_ty.as_basic_type_enum(), i32_ty_basic], false),
                None,
            ),
            elem_drop: module.add_function(
                "wasmer_vm_elem_drop",
                void_ty.fn_type(&[ctx_ptr_ty.as_basic_type_enum(), i32_ty_basic], false),
                None,
            ),
            throw_trap: module.add_function(
                "wasmer_vm_raise_trap",
                void_ty.fn_type(&[i32_ty_basic], false),
                None,
            ),

            vmfunction_import_ptr_ty: context
                .struct_type(&[i8_ptr_ty_basic, i8_ptr_ty_basic], false)
                .ptr_type(AddressSpace::Generic),
            vmfunction_import_body_element: 0,
            vmfunction_import_vmctx_element: 1,

            // TODO: this i64 is actually a rust usize
            vmmemory_definition_ptr_ty: context
                .struct_type(&[i8_ptr_ty_basic, i32_ty_basic], false)
                .ptr_type(AddressSpace::Generic),
            vmmemory_definition_base_element: 0,
            vmmemory_definition_current_length_element: 1,

            memory32_grow_ptr_ty: i32_ty
                .fn_type(
                    &[ctx_ptr_ty.as_basic_type_enum(), i32_ty_basic, i32_ty_basic],
                    false,
                )
                .ptr_type(AddressSpace::Generic),
            imported_memory32_grow_ptr_ty: i32_ty
                .fn_type(
                    &[ctx_ptr_ty.as_basic_type_enum(), i32_ty_basic, i32_ty_basic],
                    false,
                )
                .ptr_type(AddressSpace::Generic),
            memory32_size_ptr_ty: i32_ty
                .fn_type(&[ctx_ptr_ty.as_basic_type_enum(), i32_ty_basic], false)
                .ptr_type(AddressSpace::Generic),
            imported_memory32_size_ptr_ty: i32_ty
                .fn_type(&[ctx_ptr_ty.as_basic_type_enum(), i32_ty_basic], false)
                .ptr_type(AddressSpace::Generic),

            ctx_ptr_ty,
        };

        let noreturn =
            context.create_enum_attribute(Attribute::get_named_enum_kind_id("noreturn"), 0);
        intrinsics
            .throw_trap
            .add_attribute(AttributeLoc::Function, noreturn);
        intrinsics
            .func_ref
            .add_attribute(AttributeLoc::Function, intrinsics.readonly);

        intrinsics
    }
}

#[derive(Clone, Copy)]
pub enum MemoryCache<'ctx> {
    /// The memory moves around.
    Dynamic {
        ptr_to_base_ptr: PointerValue<'ctx>,
        ptr_to_current_length: PointerValue<'ctx>,
    },
    /// The memory is always in the same place.
    Static { base_ptr: PointerValue<'ctx> },
}

struct TableCache<'ctx> {
    ptr_to_base_ptr: PointerValue<'ctx>,
    ptr_to_bounds: PointerValue<'ctx>,
}

#[derive(Clone, Copy)]
pub enum GlobalCache<'ctx> {
    Mut { ptr_to_value: PointerValue<'ctx> },
    Const { value: BasicValueEnum<'ctx> },
}

#[derive(Clone)]
pub struct FunctionCache<'ctx> {
    pub func: PointerValue<'ctx>,
    pub vmctx: BasicValueEnum<'ctx>,
    pub attrs: Vec<(Attribute, AttributeLoc)>,
}

pub struct CtxType<'ctx, 'a> {
    ctx_ptr_value: PointerValue<'ctx>,

    wasm_module: &'a WasmerCompilerModule,
    cache_builder: &'a Builder<'ctx>,
    abi: &'a dyn Abi,

    cached_memories: HashMap<MemoryIndex, MemoryCache<'ctx>>,
    cached_tables: HashMap<TableIndex, TableCache<'ctx>>,
    cached_sigindices: HashMap<SignatureIndex, IntValue<'ctx>>,
    cached_globals: HashMap<GlobalIndex, GlobalCache<'ctx>>,
    cached_functions: HashMap<FunctionIndex, FunctionCache<'ctx>>,
    cached_memory_grow: HashMap<MemoryIndex, PointerValue<'ctx>>,
    cached_memory_size: HashMap<MemoryIndex, PointerValue<'ctx>>,

    offsets: VMOffsets,
}

impl<'ctx, 'a> CtxType<'ctx, 'a> {
    pub fn new(
        wasm_module: &'a WasmerCompilerModule,
        func_value: &FunctionValue<'ctx>,
        cache_builder: &'a Builder<'ctx>,
        abi: &'a dyn Abi,
    ) -> CtxType<'ctx, 'a> {
        CtxType {
            ctx_ptr_value: abi.get_vmctx_ptr_param(func_value),

            wasm_module,
            cache_builder,
            abi,

            cached_memories: HashMap::new(),
            cached_tables: HashMap::new(),
            cached_sigindices: HashMap::new(),
            cached_globals: HashMap::new(),
            cached_functions: HashMap::new(),
            cached_memory_grow: HashMap::new(),
            cached_memory_size: HashMap::new(),

            // TODO: pointer width
            offsets: VMOffsets::new(8, &wasm_module),
        }
    }

    pub fn basic(&self) -> BasicValueEnum<'ctx> {
        self.ctx_ptr_value.as_basic_value_enum()
    }

    pub fn memory(
        &mut self,
        index: MemoryIndex,
        intrinsics: &Intrinsics<'ctx>,
        module: &Module<'ctx>,
        memory_styles: &PrimaryMap<MemoryIndex, MemoryStyle>,
    ) -> MemoryCache<'ctx> {
        let (cached_memories, wasm_module, ctx_ptr_value, cache_builder, offsets) = (
            &mut self.cached_memories,
            self.wasm_module,
            self.ctx_ptr_value,
            &self.cache_builder,
            &self.offsets,
        );
        let memory_style = &memory_styles[index];
        *cached_memories.entry(index).or_insert_with(|| {
            let memory_definition_ptr =
                if let Some(local_memory_index) = wasm_module.local_memory_index(index) {
                    let offset = offsets.vmctx_vmmemory_definition(local_memory_index);
                    let offset = intrinsics.i32_ty.const_int(offset.into(), false);
                    unsafe { cache_builder.build_gep(ctx_ptr_value, &[offset], "") }
                } else {
                    let offset = offsets.vmctx_vmmemory_import(index);
                    let offset = intrinsics.i32_ty.const_int(offset.into(), false);
                    let memory_definition_ptr_ptr =
                        unsafe { cache_builder.build_gep(ctx_ptr_value, &[offset], "") };
                    let memory_definition_ptr_ptr = cache_builder
                        .build_bitcast(
                            memory_definition_ptr_ptr,
                            intrinsics.i8_ptr_ty.ptr_type(AddressSpace::Generic),
                            "",
                        )
                        .into_pointer_value();
                    let memory_definition_ptr = cache_builder
                        .build_load(memory_definition_ptr_ptr, "")
                        .into_pointer_value();
                    tbaa_label(
                        module,
                        intrinsics,
                        format!("memory {} definition", index.as_u32()),
                        memory_definition_ptr.as_instruction_value().unwrap(),
                    );
                    memory_definition_ptr
                };
            let memory_definition_ptr = cache_builder
                .build_bitcast(
                    memory_definition_ptr,
                    intrinsics.vmmemory_definition_ptr_ty,
                    "",
                )
                .into_pointer_value();
            let base_ptr = cache_builder
                .build_struct_gep(
                    memory_definition_ptr,
                    intrinsics.vmmemory_definition_base_element,
                    "",
                )
                .unwrap();
            if let MemoryStyle::Dynamic { .. } = memory_style {
                let current_length_ptr = cache_builder
                    .build_struct_gep(
                        memory_definition_ptr,
                        intrinsics.vmmemory_definition_current_length_element,
                        "",
                    )
                    .unwrap();
                MemoryCache::Dynamic {
                    ptr_to_base_ptr: base_ptr,
                    ptr_to_current_length: current_length_ptr,
                }
            } else {
                let base_ptr = cache_builder.build_load(base_ptr, "").into_pointer_value();
                tbaa_label(
                    module,
                    intrinsics,
                    format!("memory base_ptr {}", index.as_u32()),
                    base_ptr.as_instruction_value().unwrap(),
                );
                MemoryCache::Static { base_ptr }
            }
        })
    }

    fn table_prepare(
        &mut self,
        table_index: TableIndex,
        intrinsics: &Intrinsics<'ctx>,
        module: &Module<'ctx>,
    ) -> (PointerValue<'ctx>, PointerValue<'ctx>) {
        let (cached_tables, wasm_module, ctx_ptr_value, cache_builder, offsets) = (
            &mut self.cached_tables,
            self.wasm_module,
            self.ctx_ptr_value,
            &self.cache_builder,
            &self.offsets,
        );
        let TableCache {
            ptr_to_base_ptr,
            ptr_to_bounds,
        } = *cached_tables.entry(table_index).or_insert_with(|| {
            let (ptr_to_base_ptr, ptr_to_bounds) =
                if let Some(local_table_index) = wasm_module.local_table_index(table_index) {
                    let offset = intrinsics.i64_ty.const_int(
                        offsets
                            .vmctx_vmtable_definition_base(local_table_index)
                            .into(),
                        false,
                    );
                    let ptr_to_base_ptr =
                        unsafe { cache_builder.build_gep(ctx_ptr_value, &[offset], "") };
                    let ptr_to_base_ptr = cache_builder
                        .build_bitcast(
                            ptr_to_base_ptr,
                            intrinsics.i8_ptr_ty.ptr_type(AddressSpace::Generic),
                            "",
                        )
                        .into_pointer_value();
                    let offset = intrinsics.i64_ty.const_int(
                        offsets
                            .vmctx_vmtable_definition_current_elements(local_table_index)
                            .into(),
                        false,
                    );
                    let ptr_to_bounds =
                        unsafe { cache_builder.build_gep(ctx_ptr_value, &[offset], "") };
                    let ptr_to_bounds = cache_builder
                        .build_bitcast(ptr_to_bounds, intrinsics.i32_ptr_ty, "")
                        .into_pointer_value();
                    (ptr_to_base_ptr, ptr_to_bounds)
                } else {
                    let offset = intrinsics.i64_ty.const_int(
                        offsets.vmctx_vmtable_import_definition(table_index).into(),
                        false,
                    );
                    let definition_ptr_ptr =
                        unsafe { cache_builder.build_gep(ctx_ptr_value, &[offset], "") };
                    let definition_ptr_ptr = cache_builder
                        .build_bitcast(
                            definition_ptr_ptr,
                            intrinsics.i8_ptr_ty.ptr_type(AddressSpace::Generic),
                            "",
                        )
                        .into_pointer_value();
                    let definition_ptr = cache_builder
                        .build_load(definition_ptr_ptr, "")
                        .into_pointer_value();
                    tbaa_label(
                        module,
                        intrinsics,
                        format!("table {} definition", table_index.as_u32()),
                        definition_ptr.as_instruction_value().unwrap(),
                    );

                    let offset = intrinsics
                        .i64_ty
                        .const_int(offsets.vmtable_definition_base().into(), false);
                    let ptr_to_base_ptr =
                        unsafe { cache_builder.build_gep(definition_ptr, &[offset], "") };
                    let ptr_to_base_ptr = cache_builder
                        .build_bitcast(
                            ptr_to_base_ptr,
                            intrinsics.i8_ptr_ty.ptr_type(AddressSpace::Generic),
                            "",
                        )
                        .into_pointer_value();
                    let offset = intrinsics
                        .i64_ty
                        .const_int(offsets.vmtable_definition_current_elements().into(), false);
                    let ptr_to_bounds =
                        unsafe { cache_builder.build_gep(definition_ptr, &[offset], "") };
                    let ptr_to_bounds = cache_builder
                        .build_bitcast(ptr_to_bounds, intrinsics.i32_ptr_ty, "")
                        .into_pointer_value();
                    (ptr_to_base_ptr, ptr_to_bounds)
                };
            TableCache {
                ptr_to_base_ptr,
                ptr_to_bounds,
            }
        });

        (ptr_to_base_ptr, ptr_to_bounds)
    }

    pub fn table(
        &mut self,
        index: TableIndex,
        intrinsics: &Intrinsics<'ctx>,
        module: &Module<'ctx>,
    ) -> (PointerValue<'ctx>, IntValue<'ctx>) {
        let (ptr_to_base_ptr, ptr_to_bounds) = self.table_prepare(index, intrinsics, module);
        let base_ptr = self
            .cache_builder
            .build_load(ptr_to_base_ptr, "base_ptr")
            .into_pointer_value();
        let bounds = self
            .cache_builder
            .build_load(ptr_to_bounds, "bounds")
            .into_int_value();
        tbaa_label(
            module,
            intrinsics,
            format!("table_base_ptr {}", index.index()),
            base_ptr.as_instruction_value().unwrap(),
        );
        tbaa_label(
            module,
            intrinsics,
            format!("table_bounds {}", index.index()),
            bounds.as_instruction_value().unwrap(),
        );
        (base_ptr, bounds)
    }

    pub fn dynamic_sigindex(
        &mut self,
        index: SignatureIndex,
        intrinsics: &Intrinsics<'ctx>,
        module: &Module<'ctx>,
    ) -> IntValue<'ctx> {
        let (cached_sigindices, ctx_ptr_value, cache_builder, offsets) = (
            &mut self.cached_sigindices,
            self.ctx_ptr_value,
            &self.cache_builder,
            &self.offsets,
        );
        *cached_sigindices.entry(index).or_insert_with(|| {
            let byte_offset = intrinsics
                .i64_ty
                .const_int(offsets.vmctx_vmshared_signature_id(index).into(), false);
            let sigindex_ptr = unsafe {
                cache_builder.build_gep(ctx_ptr_value, &[byte_offset], "dynamic_sigindex")
            };
            let sigindex_ptr = cache_builder
                .build_bitcast(sigindex_ptr, intrinsics.i32_ptr_ty, "")
                .into_pointer_value();

            let sigindex = cache_builder
                .build_load(sigindex_ptr, "sigindex")
                .into_int_value();
            tbaa_label(
                module,
                intrinsics,
                format!("sigindex {}", index.as_u32()),
                sigindex.as_instruction_value().unwrap(),
            );
            sigindex
        })
    }

    pub fn global(
        &mut self,
        index: GlobalIndex,
        intrinsics: &Intrinsics<'ctx>,
        module: &Module<'ctx>,
    ) -> Result<&GlobalCache<'ctx>, CompileError> {
        let (cached_globals, wasm_module, ctx_ptr_value, cache_builder, offsets) = (
            &mut self.cached_globals,
            self.wasm_module,
            self.ctx_ptr_value,
            &self.cache_builder,
            &self.offsets,
        );
        Ok(match cached_globals.entry(index) {
            Entry::Occupied(entry) => entry.into_mut(),
            Entry::Vacant(entry) => {
                let global_type = wasm_module.globals[index];
                let global_value_type = global_type.ty;

                let global_mutability = global_type.mutability;
                let offset = if let Some(local_global_index) = wasm_module.local_global_index(index)
                {
                    offsets.vmctx_vmglobal_definition(local_global_index)
                } else {
                    offsets.vmctx_vmglobal_import(index)
                };
                let offset = intrinsics.i32_ty.const_int(offset.into(), false);
                let global_ptr = {
                    let global_ptr_ptr =
                        unsafe { cache_builder.build_gep(ctx_ptr_value, &[offset], "") };
                    let global_ptr_ptr = cache_builder
                        .build_bitcast(
                            global_ptr_ptr,
                            intrinsics.i32_ptr_ty.ptr_type(AddressSpace::Generic),
                            "",
                        )
                        .into_pointer_value();
                    let global_ptr = cache_builder
                        .build_load(global_ptr_ptr, "")
                        .into_pointer_value();
                    tbaa_label(
                        module,
                        intrinsics,
                        format!("global_ptr {}", index.as_u32()),
                        global_ptr.as_instruction_value().unwrap(),
                    );
                    global_ptr
                };
                let global_ptr = cache_builder
                    .build_bitcast(
                        global_ptr,
                        type_to_llvm_ptr(&intrinsics, global_value_type)?,
                        "",
                    )
                    .into_pointer_value();

                entry.insert(match global_mutability {
                    Mutability::Const => {
                        let value = cache_builder.build_load(global_ptr, "");
                        tbaa_label(
                            module,
                            intrinsics,
                            format!("global {}", index.as_u32()),
                            value.as_instruction_value().unwrap(),
                        );
                        GlobalCache::Const { value }
                    }
                    Mutability::Var => GlobalCache::Mut {
                        ptr_to_value: global_ptr,
                    },
                })
            }
        })
    }

    pub fn add_func(
        &mut self,
        function_index: FunctionIndex,
        func: PointerValue<'ctx>,
        vmctx: BasicValueEnum<'ctx>,
        attrs: &[(Attribute, AttributeLoc)],
    ) {
        match self.cached_functions.entry(function_index) {
            Entry::Occupied(_) => unreachable!("duplicate function"),
            Entry::Vacant(entry) => {
                entry.insert(FunctionCache {
                    func,
                    vmctx,
                    attrs: attrs.to_vec(),
                });
            }
        }
    }

    pub fn local_func(
        &mut self,
        _local_function_index: LocalFunctionIndex,
        function_index: FunctionIndex,
        intrinsics: &Intrinsics<'ctx>,
        module: &Module<'ctx>,
        context: &'ctx Context,
        func_type: &FuncType,
        function_name: &str,
    ) -> Result<&FunctionCache<'ctx>, CompileError> {
        let (cached_functions, ctx_ptr_value, offsets) = (
            &mut self.cached_functions,
            &self.ctx_ptr_value,
            &self.offsets,
        );
        Ok(match cached_functions.entry(function_index) {
            Entry::Occupied(entry) => entry.into_mut(),
            Entry::Vacant(entry) => {
                debug_assert!(module.get_function(function_name).is_none());
                let (llvm_func_type, llvm_func_attrs) =
                    self.abi
                        .func_type_to_llvm(context, intrinsics, Some(offsets), func_type)?;
                let func =
                    module.add_function(function_name, llvm_func_type, Some(Linkage::External));
                for (attr, attr_loc) in &llvm_func_attrs {
                    func.add_attribute(*attr_loc, *attr);
                }
                entry.insert(FunctionCache {
                    func: func.as_global_value().as_pointer_value(),
                    vmctx: ctx_ptr_value.as_basic_value_enum(),
                    attrs: llvm_func_attrs,
                })
            }
        })
    }

    pub fn func(
        &mut self,
        function_index: FunctionIndex,
        intrinsics: &Intrinsics<'ctx>,
        context: &'ctx Context,
        func_type: &FuncType,
    ) -> Result<&FunctionCache<'ctx>, CompileError> {
        let (cached_functions, wasm_module, ctx_ptr_value, cache_builder, offsets) = (
            &mut self.cached_functions,
            self.wasm_module,
            &self.ctx_ptr_value,
            &self.cache_builder,
            &self.offsets,
        );
        Ok(match cached_functions.entry(function_index) {
            Entry::Occupied(entry) => entry.into_mut(),
            Entry::Vacant(entry) => {
                let (llvm_func_type, llvm_func_attrs) =
                    self.abi
                        .func_type_to_llvm(context, intrinsics, Some(offsets), func_type)?;
                debug_assert!(wasm_module.local_func_index(function_index).is_none());
                let offset = offsets.vmctx_vmfunction_import(function_index);
                let offset = intrinsics.i32_ty.const_int(offset.into(), false);
                let vmfunction_import_ptr =
                    unsafe { cache_builder.build_gep(*ctx_ptr_value, &[offset], "") };
                let vmfunction_import_ptr = cache_builder
                    .build_bitcast(
                        vmfunction_import_ptr,
                        intrinsics.vmfunction_import_ptr_ty,
                        "",
                    )
                    .into_pointer_value();

                let body_ptr_ptr = cache_builder
                    .build_struct_gep(
                        vmfunction_import_ptr,
                        intrinsics.vmfunction_import_body_element,
                        "",
                    )
                    .unwrap();
                let body_ptr = cache_builder.build_load(body_ptr_ptr, "");
                let body_ptr = cache_builder
                    .build_bitcast(body_ptr, llvm_func_type.ptr_type(AddressSpace::Generic), "")
                    .into_pointer_value();
                let vmctx_ptr_ptr = cache_builder
                    .build_struct_gep(
                        vmfunction_import_ptr,
                        intrinsics.vmfunction_import_vmctx_element,
                        "",
                    )
                    .unwrap();
                let vmctx_ptr = cache_builder.build_load(vmctx_ptr_ptr, "");
                entry.insert(FunctionCache {
                    func: body_ptr,
                    vmctx: vmctx_ptr,
                    attrs: llvm_func_attrs,
                })
            }
        })
    }

    pub fn memory_grow(
        &mut self,
        memory_index: MemoryIndex,
        intrinsics: &Intrinsics<'ctx>,
    ) -> PointerValue<'ctx> {
        let (cached_memory_grow, wasm_module, offsets, cache_builder, ctx_ptr_value) = (
            &mut self.cached_memory_grow,
            &self.wasm_module,
            &self.offsets,
            &self.cache_builder,
            &self.ctx_ptr_value,
        );
        *cached_memory_grow.entry(memory_index).or_insert_with(|| {
            let (grow_fn, grow_fn_ty) = if wasm_module.local_memory_index(memory_index).is_some() {
                (
                    VMBuiltinFunctionIndex::get_memory32_grow_index(),
                    intrinsics.memory32_grow_ptr_ty,
                )
            } else {
                (
                    VMBuiltinFunctionIndex::get_imported_memory32_grow_index(),
                    intrinsics.imported_memory32_grow_ptr_ty,
                )
            };
            let offset = offsets.vmctx_builtin_function(grow_fn);
            let offset = intrinsics.i32_ty.const_int(offset.into(), false);
            let grow_fn_ptr_ptr = unsafe { cache_builder.build_gep(*ctx_ptr_value, &[offset], "") };

            let grow_fn_ptr_ptr = cache_builder
                .build_bitcast(
                    grow_fn_ptr_ptr,
                    grow_fn_ty.ptr_type(AddressSpace::Generic),
                    "",
                )
                .into_pointer_value();
            cache_builder
                .build_load(grow_fn_ptr_ptr, "")
                .into_pointer_value()
        })
    }

    pub fn memory_size(
        &mut self,
        memory_index: MemoryIndex,
        intrinsics: &Intrinsics<'ctx>,
    ) -> PointerValue<'ctx> {
        let (cached_memory_size, wasm_module, offsets, cache_builder, ctx_ptr_value) = (
            &mut self.cached_memory_size,
            &self.wasm_module,
            &self.offsets,
            &self.cache_builder,
            &self.ctx_ptr_value,
        );
        *cached_memory_size.entry(memory_index).or_insert_with(|| {
            let (size_fn, size_fn_ty) = if wasm_module.local_memory_index(memory_index).is_some() {
                (
                    VMBuiltinFunctionIndex::get_memory32_size_index(),
                    intrinsics.memory32_size_ptr_ty,
                )
            } else {
                (
                    VMBuiltinFunctionIndex::get_imported_memory32_size_index(),
                    intrinsics.imported_memory32_size_ptr_ty,
                )
            };
            let offset = offsets.vmctx_builtin_function(size_fn);
            let offset = intrinsics.i32_ty.const_int(offset.into(), false);
            let size_fn_ptr_ptr = unsafe { cache_builder.build_gep(*ctx_ptr_value, &[offset], "") };

            let size_fn_ptr_ptr = cache_builder
                .build_bitcast(
                    size_fn_ptr_ptr,
                    size_fn_ty.ptr_type(AddressSpace::Generic),
                    "",
                )
                .into_pointer_value();

            cache_builder
                .build_load(size_fn_ptr_ptr, "")
                .into_pointer_value()
        })
    }

    pub fn get_offsets(&self) -> &VMOffsets {
        &self.offsets
    }
}

// Given an instruction that operates on memory, mark the access as not aliasing
// other memory accesses which have a different label.
pub fn tbaa_label<'ctx>(
    module: &Module<'ctx>,
    intrinsics: &Intrinsics<'ctx>,
    label: String,
    instruction: InstructionValue<'ctx>,
) {
    // To convey to LLVM that two pointers must be pointing to distinct memory,
    // we use LLVM's Type Based Aliasing Analysis, or TBAA, to mark the memory
    // operations as having different types whose pointers may not alias.
    //
    // See the LLVM documentation at
    //   https://llvm.org/docs/LangRef.html#tbaa-metadata
    //
    // LLVM TBAA supports many features, but we use it in a simple way, with
    // only scalar types that are children of the root node. Every TBAA type we
    // declare is NoAlias with the others. See NoAlias, PartialAlias,
    // MayAlias and MustAlias in the LLVM documentation:
    //   https://llvm.org/docs/AliasAnalysis.html#must-may-and-no-alias-responses

    let context = module.get_context();

    // TODO: ContextRef can't return us the lifetime from module through Deref.
    // This could be fixed once generic_associated_types is stable.
    let context = {
        let context2 = &*context;
        unsafe { std::mem::transmute::<&Context, &'ctx Context>(context2) }
    };

    // `!wasmer_tbaa_root = {}`, the TBAA root node for wasmer.
    let tbaa_root = module
        .get_global_metadata("wasmer_tbaa_root")
        .pop()
        .unwrap_or_else(|| {
            module.add_global_metadata("wasmer_tbaa_root", &context.metadata_node(&[]));
            module.get_global_metadata("wasmer_tbaa_root")[0]
        });

    // Construct (or look up) the type descriptor, for example
    //   `!"local 0" = !{!"local 0", !wasmer_tbaa_root}`.
    let type_label = context.metadata_string(label.as_str());
    let type_tbaa = module
        .get_global_metadata(label.as_str())
        .pop()
        .unwrap_or_else(|| {
            module.add_global_metadata(
                label.as_str(),
                &context.metadata_node(&[type_label.into(), tbaa_root.into()]),
            );
            module.get_global_metadata(label.as_str())[0]
        });

    // Construct (or look up) the access tag, which is a struct of the form
    // (base type, access type, offset).
    //
    // "If BaseTy is a scalar type, Offset must be 0 and BaseTy and AccessTy
    // must be the same".
    //   -- https://llvm.org/docs/LangRef.html#tbaa-metadata
    let label = label + "_memop";
    let type_tbaa = module
        .get_global_metadata(label.as_str())
        .pop()
        .unwrap_or_else(|| {
            module.add_global_metadata(
                label.as_str(),
                &context.metadata_node(&[
                    type_tbaa.into(),
                    type_tbaa.into(),
                    intrinsics.i64_zero.into(),
                ]),
            );
            module.get_global_metadata(label.as_str())[0]
        });

    // Attach the access tag to the instruction.
    let tbaa_kind = context.get_kind_id("tbaa");
    instruction.set_metadata(type_tbaa, tbaa_kind);
}
