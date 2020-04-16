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
//
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
    let mut _ptr = data.as_ptr();
    let mut _size = length;
    let mut acc = initial as u64;
    asm!("
.intel_syntax noprefix;
# Accumulative sum.
xchg al, ah               # Swap to convert to host-bytes order.
1:
cmp rcx, 32               # If index is less than 32.
jl 2f                     # Jump to branch '2'.
add rax, [rdi]            # Sum acc with qword[0].
adc rax, [rdi + 8]        # Sum with carry qword[1].
adc rax, [rdi + 16]       # Sum with carry qword[2].
adc rax, [rdi + 24]       # Sum with carry qword[3]
adc rax, 0                # Sum carry-bit into acc.
sub rcx, 32               # Decrease index by 8.
add rdi, 32               # Jump two qwords.
jmp 1b                    # Go to beginning of loop.
2:
cmp rcx, 16               # If index is less than 16.
jl 3f                     # Jump to branch '3'.
add rax, [rdi]            # Sum acc with qword[0].
adc rax, [rdi + 8]        # Sum with carry qword[1].
adc rax, 0                # Sum carry-bit into acc.
sub rcx, 16               # Decrease index by 8.
add rdi, 16               # Jump two qwords.
3:
cmp rcx, 8                # If index is less than 8.
jl 4f                     # Jump to branch '4'.
add rax, [rdi]            # Sum acc with qword[0].
adc rax, 0                # Sum carry-bit into acc.
sub rcx, 8                # Decrease index by 8.
add rdi, 8                # Next 64-bit.
4:
cmp rcx, 4                # If index is less than 4.
jl 5f                     # Jump to branch '5'.
mov esi, dword ptr [rdi]  # Fetch 32-bit into rsi.
add rax, rsi              # Sum acc with rsi. Accumulate carry.
adc rax, 0                # Sum carry-bit into acc.
sub rcx, 4                # Decrease index by 4.
add rdi, 4                # Next 32-bit.
5:
cmp rcx, 2                # If index is less than 2.
jl 6f                     # Jump to branch '6'.
movzx rsi, word ptr [rdi] # Fetch 16-bit into rsi.
add rax, rsi              # Sum acc with rsi. Accumulate carry.
adc rax, 0                # Sum carry-bit into acc.
sub rcx, 2                # Decrease index by 2.
add rdi, 2                # Next 16-bit.
6:
cmp rcx, 1                # If index is less than 1.
jl 7f                     # Jump to branch '7'.
movzx rsi, byte ptr [rdi] # Fetch 8-bit into rsi.
add rax, rsi              # Sum acc with rsi. Accumulate carry.
adc rax, 0                # Sum carry-bit into acc.
# Fold 64-bit into 16-bit.
7:
mov rsi, rax              # Assign acc to rsi.
shr rsi, 32               # Shift rsi 32-bit. Stores higher part of acc.
mov eax, eax              # Clear out higher-part of rax. Stores lower part of acc.
add eax, esi              # 32-bit sum of acc and rsi.
adc eax, 0                # Sum carry to acc.
mov esi, eax              # Repeat for 16-bit.
shr esi, 16
and eax, 0x0000ffff
add ax, si
adc ax, 0
# One's complement.
not eax                   # One-complement of eax.
and eax, 0xffff           # Clear out higher part of eax.
# Swap.
xchg al, ah
"
         :/* outputs */ "={ax}"(acc), "={rdi}"(_ptr), "={rcx}"(_size)
         :/* inputs */ "0"(acc), "1"(_ptr), "2"(_size)
         :/* clobbers */ "rsi"
         :/* options */ "volatile"
    );
    acc as u16
}

#[cfg(target_arch="aarch64")]
unsafe fn checksum(data: &[u8], length: usize, initial: u16) -> u16 {
    let mut _ptr = data.as_ptr();
    let mut _size = length;
    let mut acc = initial as u64;
    // Accumulative sum (x0: initial/acc, x1/2: tmp, x3: data, x4: size)
    asm!("
ands x5, x4, ~31
rev16 w0, w0          // Swap initial to convert to host-bytes order.
b.eq 2f               // skip 32 bytes at once block, carry flag cleared

1:
ldp x1, x2, [x3], 16  // Load dword[0..1] and advance input
adds x0, x0, x1       // Sum acc with dword[0].
adcs x0, x0, x2       // Sum with carry dword[1].
ldp x1, x2, [x3], 16  // Load dword[2..3] and advance input
adcs x0, x0, x1       // Sum with carry dword[2].
adcs x0, x0, x2       // Sum with carry dword[3].
adc x0, x0, xzr       // Sum carry-bit into acc.
subs x5, x5, 32       // Consume four dwords.
b.gt 1b
tst x5, 32            // clear carry flag

2:
tbz x4, 4, 3f         // skip 16 bytes at once block
ldp x1, x2, [x3], 16  // Load dword[0..1] and advance
adds x0, x0, x1       // Sum with carry dword[0].
adcs x0, x0, x2       // Sum with carry dword[1].

3:
tbz x4, 3, 4f         // skip 8 bytes at once block
ldr x2, [x3], 8       // Load dword and advance
adcs x0, x0, x2       // Sum acc with dword[0]. Accumulate carry.

4:
tbz x4, 2, 5f         // skip 4 bytes at once block
ldr w1, [x3], 4       // Load word and advance
adcs x0, x0, x1       // Sum acc with word[0]. Accumulate carry.

5:
tbz x4, 1, 6f         // skip 2 bytes at once block
ldrh w1, [x3], 2      // Load hword and advance
adcs x0, x0, x1       // Sum acc with hword[0]. Accumulate carry.

6:
tbz x4, 0, 7f         // If size is less than 1.
ldrb w1, [x3]         // Load byte.
adcs x0, x0, x1       // Sum acc with byte. Accumulate carry.

// Fold 64-bit into 16-bit.
7:
lsr x1, x0, 32        // Store high 32 bit of acc in x1.
adcs w0, w0, w1       // 32-bit sum of acc and r1. Accumulate carry.
adc w0, w0, wzr       // Sum carry to acc.
uxth w2, w0           // Repeat for 16-bit.
add w0, w2, w0, lsr 16
add w0, w0, w0, lsr 16 // (This sums the carry, if any, into acc.)
// One's complement.
mvn w0, w0
// Swap.
rev16 w0, w0
"
         :/* outputs */ "={x0}"(acc), "={x3}"(_ptr), "={x4}"(_size)
         :/* inputs */ "0"(acc), "1"(_ptr), "2"(_size)
         :/* clobbers */ "x1", "x2", "x5"
         :/* options */ "volatile"
    );
    acc as u16
}

#[cfg(test)]
mod selftest {
    use super::*;

    #[test]
    fn checksum() {
        let cases: Vec<&[u8]> = vec![
            &[0xffu8, 0xff, 0xff, 0xff, 0xff],
            &[0u8, 0, 0, 0, 0],
            &[42u8, 41, 40, 39, 38, 37, 36, 35, 34, 33, 32, 31, 30, 29, 28],
            &[],
            &[01u8, 02, 03, 04, 05, 06, 07, 08, 09, 10, 11, 12, 13, 14, 15, 16,
              01u8, 02, 03, 04, 05, 06, 07, 08, 09, 10, 11, 12, 13, 14, 15, 16,
              01u8, 02, 03, 04, 05, 06, 07, 08, 09, 10, 11, 12, 13, 14, 15, 16,
              01u8, 02, 03, 04, 05, 06, 07, 08, 09, 10, 11, 12, 13, 14, 15],
            &[0xFFu8, 0xFFu8, 0xFFu8, 0xFFu8, 0xFFu8, 0xFFu8, 0xFFu8, 0xFFu8,
              0x01u8],
        ];
        for case in cases {
            for l in 0..=case.len() {
                let n = checksum_rust(&case, l);
                println!("{:?} {} {}", &case, l, n);
                assert_eq!(ipsum(&case, l, 0), n);
            }
        }
    }

    #[test]
    fn checksum_random() {
        let mut progress = 1;
        for i in 1..=32 { // Crank this up to run more random test cases
            if i >= progress {
                println!("{}", progress);
                progress *= 2;
            }
            for l in 0..=1500 { // Tune this down (to e.g. 63) for faster cases
                let mut case = vec![0u8; l];
                lib::random_bytes(&mut case, l);
                let r = checksum_rust(&case, l);
                let n = ipsum(&case, l, 0);
                if r != n {
                    println!("{:?} len={} ref={} asm={}", &case, l, r, n);
                    panic!("mismatch");
                }
            }
        }
    }

    #[test]
    fn checksum_bench() {
        let nchunks = match std::env::var("RUSH_CHECKSUM_NCHUNKS") {
            Ok(val) => val.parse::<f64>().unwrap() as usize,
            _ => 1_000_000
        };
        let chunksize = match std::env::var("RUSH_CHECKSUM_CHUNKSIZE") {
            Ok(val) => val.parse::<usize>().unwrap(),
            _ => 60
        };
        let case = vec![0u8; nchunks];
        let mut acc = 0;
        for _ in 1..=nchunks {
            acc += ipsum(&case, chunksize, 0) as usize;
        }
        assert_eq!(acc, nchunks * 65535);
        println!("Checksummed {} * {} byte chunks", nchunks, chunksize);
    }

}
