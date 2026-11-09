use std::fs::File;
use std::io::Read;
use std::path::Path;

#[repr(C)]
struct CcSha256Ctx {
    count: [u32; 2],
    hash: [u32; 8],
    wbuf: [u32; 16],
}

extern "C" {
    fn CC_SHA256_Init(ctx: *mut CcSha256Ctx) -> i32;
    fn CC_SHA256_Update(ctx: *mut CcSha256Ctx, data: *const u8, len: u32) -> i32;
    fn CC_SHA256_Final(md: *mut u8, ctx: *mut CcSha256Ctx) -> i32;
}

pub fn sha256_file(path: &Path) -> Result<String, std::io::Error> {
    let mut file = File::open(path)?;
    let mut ctx = CcSha256Ctx {
        count: [0; 2],
        hash: [0; 8],
        wbuf: [0; 16],
    };
    if unsafe { CC_SHA256_Init(&mut ctx) } != 1 {
        return Err(std::io::Error::other("sha256 init failed"));
    }
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        if unsafe { CC_SHA256_Update(&mut ctx, buf.as_ptr(), n as u32) } != 1 {
            return Err(std::io::Error::other("sha256 update failed"));
        }
    }
    let mut out = [0u8; 32];
    if unsafe { CC_SHA256_Final(out.as_mut_ptr(), &mut ctx) } != 1 {
        return Err(std::io::Error::other("sha256 final failed"));
    }
    Ok(out.iter().map(|byte| format!("{byte:02x}")).collect())
}
