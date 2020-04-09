use super::lib;

// IP CHECKSUM
//
// The checksum module provides an optimized ones-complement checksum
// routine.
//
//  ipsum(data: &[u8], length: usize, initial: u16) -> checksum: u16
//    return the ones-complement checksum for the given region of memory

// Reference implementation in Rust.
fn checksum_rust(data: &[u8], length: usize) -> u16 {
    let ptr: *const u8 = data.as_ptr();
    let mut csum: u64 = 0;
    let mut i = length;
    while i > 1 {
        let word = unsafe { *(ptr.offset((length-i) as isize) as *const u16) };
        csum += word as u64;
        i -= 2;
    }
    if i == 1 {
        csum += data[length-1] as u64;
    }
    loop {
        let carry = csum >> 16;
        if carry == 0 { break; }
        csum = (csum & 0xffff) + carry;
    }
    lib::ntohs(!csum as u16 & 0xffff)
}

// ipsum: return the ones-complement checksum for the given region of memory

// data is a byte slice to be checksummed.
// initial is an unsigned 16-bit number in host byte order which is used as
// the starting value of the accumulator. 
// The result is the IP checksum over the data in host byte order.
// 
// The 'initial' argument can be used to verify a checksum or to calculate the
// checksum in an incremental manner over chunks of memory. The synopsis to
// check whether the checksum over a block of data is equal to a given value is
// the following
//
//   if ipsum(data, len, value) == 0 {
//       checksum correct
//   } else {
//       checksum incorrect
//   }
//
// To chain the calculation of checksums over multiple blocks of data together
// to obtain the overall checksum, one needs to pass the one's complement of
// the checksum of one block as initial value to the call of ipsum() for the
// following block, e.g.
//
//   let sum1 = ipsum(data1, length1, 0);
//   let total_sum = ipsum(data2, length2, !sum1);
//
pub fn ipsum(data: &[u8], length: usize, initial: u16) -> u16 {
    unsafe { checksum(data, length, initial) }
}

#[cfg(target_arch="x86_64")]
unsafe fn checksum(data: &[u8], length: usize, initial: u16) -> u16 {
    let ptr: *const u8 = data.as_ptr();
    let initial = initial as u64;
    let csum: u16;
    asm!("
.intel_syntax noprefix;
# Accumulative sum.
mov rax, rdx                # Dx (3rd argument: initial).
xchg al, ah                 # Swap to convert to host-bytes order.
mov rcx, rsi                # Rsi (2nd argument: size).
xor r9, r9                  # Clear out r9. Stores value of array.
xor r8, r8                  # Clear out r8. Stores array index.
1:
cmp rcx, 32                 # If index is less than 32.
jl 2f                       # Jump to branch '2'.
add rax, [rdi + r8]         # Sum acc with qword[0].
adc rax, [rdi + r8 + 8]     # Sum with carry qword[1].
adc rax, [rdi + r8 + 16]    # Sum with carry qword[2].
adc rax, [rdi + r8 + 24]    # Sum with carry qword[3]
adc rax, 0                  # Sum carry-bit into acc.
sub rcx, 32                 # Decrease index by 8.
add r8, 32                  # Jump two qwords.
jmp 1b                      # Go to beginning of loop.
2:
cmp rcx, 16                 # If index is less than 16.
jl 3f                       # Jump to branch '3'.
add rax, [rdi + r8]         # Sum acc with qword[0].
adc rax, [rdi + r8 + 8]     # Sum with carry qword[1].
adc rax, 0                  # Sum carry-bit into acc.
sub rcx, 16                 # Decrease index by 8.
add r8, 16                  # Jump two qwords.
3:
cmp rcx, 8                  # If index is less than 8.
jl 4f                       # Jump to branch '4'.
add rax, [rdi + r8]         # Sum acc with qword[0].
adc rax, 0                  # Sum carry-bit into acc.
sub rcx, 8                  # Decrease index by 8.
add r8, 8                   # Next 64-bit.
4:
cmp rcx, 4                  # If index is less than 4.
jl 5f                       # Jump to branch '5'.
mov r9d, dword ptr [rdi+r8] # Fetch 32-bit from data + r8 into r9d.
add rax, r9                 # Sum acc with r9. Accumulate carry.
sub rcx, 4                  # Decrease index by 4.
add r8, 4                   # Next 32-bit.
5:
cmp rcx, 2                  # If index is less than 2.
jl 6f                       # Jump to branch '6'.
movzx r9, word ptr [rdi+r8] # Fetch 16-bit from data + r8 into r9.
add rax, r9                 # Sum acc with r9. Accumulate carry.
sub rcx, 2                  # Decrease index by 2.
add r8, 2                   # Next 16-bit.
6:
cmp rcx, 1                  # If index is less than 1.
jl 7f                       # Jump to branch '7'.
movzx r9, byte ptr [rdi+r8] # Fetch 8-bit from data + r8 into r9.
add rax, r9                 # Sum acc with r9. Accumulate carry.
# Fold 64-bit into 16-bit.
7:
mov r9, rax                 # Assign acc to r9.
shr r9, 32                  # Shift r9 32-bit. Stores higher part of acc.
mov eax, eax                # Clear out higher-part of rax. Stores lower part of acc.
add eax, r9d                # 32-bit sum of acc and r9.
adc eax, 0                  # Sum carry to acc.
mov r9d, eax                # Repeat for 16-bit.
shr r9d, 16
and eax, 0x0000ffff
add ax, r9w
adc ax, 0
# One's complement.
not eax                     # One-complement of eax.
and eax, 0xffff             # Clear out higher part of eax.
# Swap.
xchg al, ah
"
        :// outputs
        "={ax}"(csum)
        :// inputs
        "{rdi}"(ptr), "{rsi}"(length), "{rdx}"(initial)
        :// clobbers
        "rcx", "r8", "r9"
     
    );
    csum
}

#[cfg(test)]
mod selftest {
    use super::*;

    #[test]
    fn checksum() {
        let cases: Vec<&[u8]> = vec![
            &[0u8, 0, 0, 0, 0],
            &[42u8, 41, 40, 39, 38, 37, 36, 35, 34, 33, 32, 31, 30, 29, 28],
            &[],
        ];
        for case in cases {
            let n = ipsum(&case, case.len(), 0);
            println!("{:?} {} {}", &case, case.len(), n);
            assert_eq!(n, checksum_rust(&case, case.len()));
        }
    }

}
