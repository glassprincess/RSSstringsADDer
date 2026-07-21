use std::io::{self, Write};
use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;

type HANDLE = *mut std::ffi::c_void;
type DWORD = u32;
type SIZE_T = usize;
type BOOL = i32;

const NULL: HANDLE = 0isize as HANDLE;
const INVALID_HANDLE_VALUE: HANDLE = -1isize as HANDLE;
const TH32CS_SNAPPROCESS: DWORD = 0x00000002;
const MAX_PATH: usize = 260;
const PROCESS_ALL_ACCESS: DWORD = 0x1F0FFF;
const MEM_COMMIT: DWORD = 0x1000;
const MEM_RESERVE: DWORD = 0x2000;
const MEM_RELEASE: DWORD = 0x8000;
const PAGE_READWRITE: DWORD = 0x04;

#[repr(C)]
struct PROCESSENTRY32W {
    dwSize: DWORD,
    cntUsage: DWORD,
    th32ProcessID: DWORD,
    th32DefaultHeapID: usize,
    th32ModuleID: DWORD,
    cntThreads: DWORD,
    th32ParentProcessID: DWORD,
    pcPriClassBase: i32,
    dwFlags: DWORD,
    szExeFile: [u16; MAX_PATH],
}

#[link(name = "kernel32")]
extern "system" {
    fn CreateToolhelp32Snapshot(dwFlags: DWORD, th32ProcessID: DWORD) -> HANDLE;
    fn Process32FirstW(hSnapshot: HANDLE, lppe: *mut PROCESSENTRY32W) -> BOOL;
    fn Process32NextW(hSnapshot: HANDLE, lppe: *mut PROCESSENTRY32W) -> BOOL;
    fn CloseHandle(hObject: HANDLE) -> BOOL;
    fn OpenProcess(dwDesiredAccess: DWORD, bInheritHandle: BOOL, dwProcessId: DWORD) -> HANDLE;
    fn VirtualAllocEx(
        hProcess: HANDLE,
        lpAddress: *const std::ffi::c_void,
        dwSize: SIZE_T,
        flAllocationType: DWORD,
        flProtect: DWORD,
    ) -> *mut std::ffi::c_void;
    fn VirtualFreeEx(
        hProcess: HANDLE,
        lpAddress: *const std::ffi::c_void,
        dwSize: SIZE_T,
        dwFreeType: DWORD,
    ) -> BOOL;
    fn WriteProcessMemory(
        hProcess: HANDLE,
        lpBaseAddress: *mut std::ffi::c_void,
        lpBuffer: *const std::ffi::c_void,
        nSize: SIZE_T,
        lpNumberOfBytesWritten: *mut SIZE_T,
    ) -> BOOL;
}

fn from_wide(wide: &[u16]) -> String {
    let len = wide.iter().position(|&c| c == 0).unwrap_or(wide.len());
    String::from_utf16_lossy(&wide[..len])
}

fn find_javaw_processes() -> Vec<(DWORD, String)> {
    let mut results = Vec::new();

    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot == INVALID_HANDLE_VALUE || snapshot == NULL {
            return results;
        }

        let mut entry: PROCESSENTRY32W = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as DWORD;

        if Process32FirstW(snapshot, &mut entry) == 1 {
            loop {
                let name = from_wide(&entry.szExeFile);
                let lower = name.to_lowercase();
                if lower == "javaw.exe" || lower == "java.exe" {
                    results.push((entry.th32ProcessID, name));
                }
                if Process32NextW(snapshot, &mut entry) == 0 {
                    break;
                }
            }
        }

        CloseHandle(snapshot);
    }

    results
}

fn read_line() -> String {
    let mut input = String::new();
    io::stdin().read_line(&mut input).ok();
    input.trim().to_string()
}

fn main() {
    println!();
    println!("============================================");
    println!("          rssstringadder v1.0              ");
    println!("============================================");
    println!();

    // --- Поиск процессов ---
    println!("[*] Ищу java процессы...");
    let processes = find_javaw_processes();

    if processes.is_empty() {
        println!("[-] Не нашёл ни одного java.exe / javaw.exe");
        println!("[*] Запусти Minecraft и попробуй снова.");
        println!();
        print!("Нажми Enter для выхода...");
        let _ = io::stdout().flush();
        let _ = read_line();
        return;
    }

    println!("[+] Найдено процессов: {}", processes.len());
    println!();

    for (i, (pid, name)) in processes.iter().enumerate() {
        println!("  [{}] {} (PID: {})", i, name, pid);
    }

    println!();

    // --- Выбор процесса ---
    let choice = loop {
        print!("Выбери номер процесса: ");
        let _ = io::stdout().flush();
        let input = read_line();
        if let Ok(idx) = input.parse::<usize>() {
            if idx < processes.len() {
                break idx;
            }
        }
        println!("[-] Неверно. Введи число от 0 до {}", processes.len() - 1);
    };

    let (pid, proc_name) = &processes[choice];
    println!();
    println!("[+] Выбран: {} (PID: {})", proc_name, pid);

    // --- Открытие процесса ---
    let handle = unsafe { OpenProcess(PROCESS_ALL_ACCESS, 0, *pid) };
    if handle == NULL {
        println!("[-] Не удалось открыть процесс. Запусти от администратора.");
        print!("Нажми Enter для выхода...");
        let _ = io::stdout().flush();
        let _ = read_line();
        return;
    }

    println!("[+] Процесс открыт успешно.");
    println!();

    // --- Ввод стринга ---
    print!("Введи строку для добавления: ");
    let _ = io::stdout().flush();
    let text = read_line();

    if text.is_empty() {
        println!("[-] Пустая строка. Выход.");
        unsafe { CloseHandle(handle); }
        return;
    }

    let text_bytes = text.as_bytes();
    let alloc_size = text_bytes.len() + 1;

    println!();

    // --- Ввод количества ---
    let count = loop {
        print!("Сколько копий добавить?: ");
        let _ = io::stdout().flush();
        let input = read_line();
        if let Ok(n) = input.parse::<usize>() {
            if n > 0 {
                break n;
            }
        }
        println!("[-] Введи число больше 0.");
    };

    println!();
    println!("[*] Добавляю {} копий строки \"{}\"...", count, text);
    println!();

    // --- Выделение памяти и запись ---
    let mut addresses: Vec<usize> = Vec::new();
    let mut success = 0;

    for i in 0..count {
        unsafe {
            let addr = VirtualAllocEx(
                handle,
                std::ptr::null(),
                alloc_size,
                MEM_COMMIT | MEM_RESERVE,
                PAGE_READWRITE,
            );

            if addr.is_null() {
                println!("  [{}/{}] Ошибка выделения памяти", i + 1, count);
                continue;
            }

            let mut written: SIZE_T = 0;
            let result = WriteProcessMemory(
                handle,
                addr,
                text_bytes.as_ptr() as *const std::ffi::c_void,
                alloc_size,
                &mut written,
            );

            if result == 0 || written != alloc_size {
                println!("  [{}/{}] Ошибка записи", i + 1, count);
                VirtualFreeEx(handle, addr, 0, MEM_RELEASE);
                continue;
            }

            let addr_usize = addr as usize;
            addresses.push(addr_usize);
            println!("  [{}/{}] OK — 0x{:016X}", i + 1, count, addr_usize);
            success += 1;
        }
    }

    println!();
    println!("[+] Готово! Успешно добавлено: {}/{}", success, count);
    println!("[*] Адреса (копируй нужные):");
    println!();

    for (i, addr) in addresses.iter().enumerate() {
        println!("  [{}] 0x{:016X}", i, addr);
    }

    println!();

    // --- Завершение ---
    unsafe {
        // Не освобождаем память — пусть стринги остаются в процессе
        CloseHandle(handle);
    }

    println!("[+] Хэндл закрыт. Строки остались в памяти процесса.");
    println!();
    print!("Нажми Enter для выхода...");
    let _ = io::stdout().flush();
    let _ = read_line();
}