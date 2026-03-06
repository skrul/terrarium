//! mDNS A record registration via macOS Bonjour (dns_sd.h).
//!
//! Registers `{project}-terrarium.local` A records pointing to the Mac's LAN IP.
//! Uses `DNSServiceCreateConnection` + `DNSServiceRegisterRecord` for direct
//! record registration without advertising a service.

use std::collections::HashMap;
use std::ffi::CString;
use std::net::Ipv4Addr;
use std::os::raw::{c_char, c_void};
use std::ptr;
use std::sync::Mutex;

// dns_sd.h types
type DNSServiceRef = *mut c_void;
type DNSRecordRef = *mut c_void;
type DNSServiceFlags = u32;
type DNSServiceErrorType = i32;

const K_DNS_SERVICE_ERR_NO_ERROR: DNSServiceErrorType = 0;
const K_DNS_SERVICE_FLAGS_UNIQUE: DNSServiceFlags = 0x20;
const K_DNS_SERVICE_TYPE_A: u16 = 1;
const K_DNS_SERVICE_CLASS_IN: u16 = 1;

type DNSServiceRegisterRecordReply = Option<
    unsafe extern "C" fn(
        sd_ref: DNSServiceRef,
        record_ref: DNSRecordRef,
        flags: DNSServiceFlags,
        error_code: DNSServiceErrorType,
        context: *mut c_void,
    ),
>;

#[link(name = "System", kind = "dylib")]
extern "C" {
    fn DNSServiceCreateConnection(sd_ref: *mut DNSServiceRef) -> DNSServiceErrorType;

    fn DNSServiceRegisterRecord(
        sd_ref: DNSServiceRef,
        record_ref: *mut DNSRecordRef,
        flags: DNSServiceFlags,
        interface_index: u32,
        fullname: *const c_char,
        rrtype: u16,
        rrclass: u16,
        rdlen: u16,
        rdata: *const c_void,
        ttl: u32,
        callback: DNSServiceRegisterRecordReply,
        context: *mut c_void,
    ) -> DNSServiceErrorType;

    fn DNSServiceRemoveRecord(
        sd_ref: DNSServiceRef,
        record_ref: DNSRecordRef,
        flags: DNSServiceFlags,
    ) -> DNSServiceErrorType;

    fn DNSServiceRefSockFD(sd_ref: DNSServiceRef) -> i32;
    fn DNSServiceProcessResult(sd_ref: DNSServiceRef) -> DNSServiceErrorType;
    fn DNSServiceRefDeallocate(sd_ref: DNSServiceRef);
}

unsafe extern "C" fn register_record_callback(
    _sd_ref: DNSServiceRef,
    _record_ref: DNSRecordRef,
    _flags: DNSServiceFlags,
    error_code: DNSServiceErrorType,
    _context: *mut c_void,
) {
    if error_code != K_DNS_SERVICE_ERR_NO_ERROR {
        eprintln!("mDNS record registration callback error: {}", error_code);
    }
}

/// Get the host's LAN IPv4 address using getifaddrs.
fn get_lan_ip() -> Option<Ipv4Addr> {
    use libc::{freeifaddrs, getifaddrs, ifaddrs, sockaddr_in, AF_INET, IFF_LOOPBACK, IFF_UP};

    unsafe {
        let mut addrs: *mut ifaddrs = ptr::null_mut();
        if getifaddrs(&mut addrs) != 0 {
            return None;
        }

        let mut result = None;
        let mut current = addrs;

        while !current.is_null() {
            let ifa = &*current;
            let flags = ifa.ifa_flags as i32;

            if (flags & IFF_UP) != 0
                && (flags & IFF_LOOPBACK) == 0
                && !ifa.ifa_addr.is_null()
                && (*ifa.ifa_addr).sa_family as i32 == AF_INET
            {
                let addr = &*(ifa.ifa_addr as *const sockaddr_in);
                let ip = Ipv4Addr::from(u32::from_be(addr.sin_addr.s_addr));

                // Prefer 192.168.x.x or 10.x.x.x or 172.16-31.x.x
                if ip.is_private() {
                    result = Some(ip);
                    // Prefer en0 (Wi-Fi/Ethernet) over other interfaces
                    let name = std::ffi::CStr::from_ptr(ifa.ifa_name);
                    if let Ok(name_str) = name.to_str() {
                        if name_str == "en0" {
                            break;
                        }
                    }
                }
            }

            current = ifa.ifa_next;
        }

        freeifaddrs(addrs);
        result
    }
}

struct MdnsRecord {
    record_ref: DNSRecordRef,
}

pub struct MdnsRegistrar {
    connection: DNSServiceRef,
    /// hostname -> MdnsRecord
    records: Mutex<HashMap<String, MdnsRecord>>,
    /// Background thread handle for processing events
    _event_thread: Option<std::thread::JoinHandle<()>>,
}

// DNSServiceRef is thread-safe (documented in dns_sd.h)
unsafe impl Send for MdnsRegistrar {}
unsafe impl Sync for MdnsRegistrar {}

impl MdnsRegistrar {
    /// Create a new mDNS registrar with a shared Bonjour connection.
    pub fn new() -> Result<Self, String> {
        let mut connection: DNSServiceRef = ptr::null_mut();

        let err = unsafe { DNSServiceCreateConnection(&mut connection) };
        if err != K_DNS_SERVICE_ERR_NO_ERROR {
            return Err(format!(
                "DNSServiceCreateConnection failed with error {}",
                err
            ));
        }

        // Get the socket fd and spawn a thread to process events.
        // This keeps registrations alive.
        let fd = unsafe { DNSServiceRefSockFD(connection) };

        // Pass the connection as a usize to satisfy Send requirements.
        // This is safe because dns_sd.h documents that DNSServiceRef is thread-safe
        // for DNSServiceProcessResult calls.
        let conn_addr = connection as usize;

        let event_thread = std::thread::spawn(move || {
            let conn = conn_addr as DNSServiceRef;
            loop {
                // Use select() to wait for data on the socket
                unsafe {
                    let mut read_fds: libc::fd_set = std::mem::zeroed();
                    libc::FD_SET(fd, &mut read_fds);

                    let mut timeout = libc::timeval {
                        tv_sec: 1,
                        tv_usec: 0,
                    };

                    let n = libc::select(
                        fd + 1,
                        &mut read_fds,
                        ptr::null_mut(),
                        ptr::null_mut(),
                        &mut timeout,
                    );

                    if n > 0 {
                        let err = DNSServiceProcessResult(conn);
                        if err != K_DNS_SERVICE_ERR_NO_ERROR {
                            // Connection was deallocated or error occurred
                            break;
                        }
                    } else if n < 0 {
                        // select error — connection likely deallocated
                        break;
                    }
                    // n == 0 is timeout, just loop
                }
            }
        });

        Ok(Self {
            connection,
            records: Mutex::new(HashMap::new()),
            _event_thread: Some(event_thread),
        })
    }

    /// Format a project name into an mDNS hostname.
    pub fn hostname_for_project(project_name: &str) -> String {
        format!("{}-terrarium.local", project_name)
    }

    /// Register an A record for `{project_name}-terrarium.local` pointing to the LAN IP.
    pub fn register(&self, project_name: &str) -> Result<String, String> {
        let hostname = Self::hostname_for_project(project_name);

        let ip = get_lan_ip().ok_or_else(|| "Could not determine LAN IP address".to_string())?;

        let fullname =
            CString::new(hostname.clone()).map_err(|e| format!("Invalid hostname: {}", e))?;

        let ip_bytes = ip.octets();
        let mut record_ref: DNSRecordRef = ptr::null_mut();

        let err = unsafe {
            DNSServiceRegisterRecord(
                self.connection,
                &mut record_ref,
                K_DNS_SERVICE_FLAGS_UNIQUE,
                0, // all interfaces
                fullname.as_ptr(),
                K_DNS_SERVICE_TYPE_A,
                K_DNS_SERVICE_CLASS_IN,
                4, // IPv4 = 4 bytes
                ip_bytes.as_ptr() as *const c_void,
                120, // TTL in seconds
                Some(register_record_callback),
                ptr::null_mut(),
            )
        };

        if err != K_DNS_SERVICE_ERR_NO_ERROR {
            return Err(format!(
                "DNSServiceRegisterRecord failed with error {}",
                err
            ));
        }

        self.records
            .lock()
            .unwrap()
            .insert(hostname.clone(), MdnsRecord { record_ref });

        eprintln!(
            "mDNS: registered {} -> {}",
            hostname, ip
        );

        Ok(hostname)
    }

    /// Deregister the A record for a project.
    pub fn deregister(&self, project_name: &str) -> Result<(), String> {
        let hostname = Self::hostname_for_project(project_name);

        let record = self.records.lock().unwrap().remove(&hostname);

        if let Some(record) = record {
            let err =
                unsafe { DNSServiceRemoveRecord(self.connection, record.record_ref, 0) };

            if err != K_DNS_SERVICE_ERR_NO_ERROR {
                return Err(format!(
                    "DNSServiceRemoveRecord failed with error {}",
                    err
                ));
            }

            eprintln!("mDNS: deregistered {}", hostname);
        }

        Ok(())
    }

    /// Check if a hostname is currently registered.
    pub fn is_registered(&self, project_name: &str) -> bool {
        let hostname = Self::hostname_for_project(project_name);
        self.records.lock().unwrap().contains_key(&hostname)
    }
}

impl Drop for MdnsRegistrar {
    fn drop(&mut self) {
        // Remove all records first
        let records: Vec<(String, MdnsRecord)> =
            self.records.lock().unwrap().drain().collect();

        for (hostname, record) in records {
            unsafe {
                let _ = DNSServiceRemoveRecord(self.connection, record.record_ref, 0);
            }
            eprintln!("mDNS: deregistered {} (shutdown)", hostname);
        }

        // Deallocate the connection (this will also cause the event thread to exit)
        unsafe {
            DNSServiceRefDeallocate(self.connection);
        }
    }
}
