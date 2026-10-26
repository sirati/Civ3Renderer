//! The probe restores ESP even if the callee pops the wrong number of words,
//! then Rust reports the mismatch. Test process only, one thread at a time.
static mut ABI_STATE: [u32; 6] = [0; 6];
core::arch::global_asm!(r#"
    .text
    .globl _mill_abi_probe
_mill_abi_probe:
    push ebp
    push ebx
    push esi
    push edi
    mov dword ptr [{state}], esp
    mov eax, dword ptr [esp + 20]
    mov ecx, dword ptr [esp + 24]
    mov edi, dword ptr [esp + 28]
    mov edx, dword ptr [esp + 32]
    test edx, edx
    jz 3f
2:
    dec edx
    push dword ptr [edi + edx * 4]
    test edx, edx
    jnz 2b
3:
    mov ebx, 0x12345678
    mov esi, 0x23456789
    mov edi, 0x3456789a
    mov ebp, 0x456789ab
    call eax
    mov dword ptr [{state} + 4], esp
    mov dword ptr [{state} + 8], ebx
    mov dword ptr [{state} + 12], esi
    mov dword ptr [{state} + 16], edi
    mov dword ptr [{state} + 20], ebp
    mov esp, dword ptr [{state}]
    pop edi
    pop esi
    pop ebx
    pop ebp
    ret
"#, state = sym ABI_STATE);
unsafe extern "C" {
    fn mill_abi_probe(f: u32, this: u32, args: *const u32, count: u32) -> u32;
}

pub unsafe fn function(f: u32, this: u32, args: &[u32]) -> u32 {
    let answer = unsafe { mill_abi_probe(f, this, args.as_ptr(), args.len() as u32) };
    let state = unsafe { std::ptr::read_volatile(&raw const ABI_STATE) };
    assert_eq!(
        state[0],
        state[1],
        "callee stack cleanup at {f:x}, {} words",
        args.len()
    );
    assert_eq!(
        &state[2..],
        &[0x12345678, 0x23456789, 0x3456789a, 0x456789ab],
        "callee-saved registers at {f:x}"
    );
    answer
}
