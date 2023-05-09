use crate::simulator::SimErr;

use rustc_hash::FxHashMap;
use std::collections::VecDeque;
use rand::Rng;

/// Size of physical pages allocated to programs
pub const PAGE_SIZE: usize = 4096;

/// Number of entries in page-table levels. The ratio has an inverse relation-ship to page-sizes
pub const PAGE_TABLE_ENTRIES: usize = PAGE_SIZE / 4;

/// Stall-time in cycles if an access to Ram occurs
pub const RAM_STALL: usize = 100;

/// Stall-time in cycles if an access to L1 Cache occurs
pub const L1_CACHE_STALL: usize = 10;

/// Wrapper around virtual addresses
#[derive(Debug, Default, Clone, Copy, Eq, Hash, PartialEq)]
pub struct VAddr(pub u32);

/// Wrapper around physical addresses
#[derive(Debug, Clone, Copy, Eq, Hash, PartialEq)]
pub struct PAddr(pub u32);

/// Permission bits as represented on the page tables
#[non_exhaustive]
#[derive(Debug, Clone, Copy)]
pub struct Perms;
impl Perms {
    pub const UNSET: u8 = 0;
    pub const EXEC:  u8 = 1;
    pub const WRITE: u8 = 2;
    pub const READ:  u8 = 4;
}

/// Represents a cache-line that contains 32 DWords of memory
#[derive(Debug, Clone)]
pub struct CacheLine {
    /// Bit used to determine if the data in this cacheline is valid or has been invalidated
    pub is_valid: bool,

    /// 21 tag bits
    pub tag: u32,

    /// Data-backing for 16-Dword entries in a cacheline
    pub data: Vec<u8>,
}

impl Default for CacheLine {
    /// Empty invalidated cacheline
    fn default() -> Self {
        Self {
            is_valid: false,
            tag: 0,
            data: vec![0u8; 64],
        }
    }
}

#[derive(Debug, Clone)]
/// This takes care of managing memory and related structures such as caches or page-tables
pub struct Mmu {
    /// Since we don't just want to allocate 2**32 bytes of memory, we use a hashmap to pull pages
    /// out of memory after getting the correct physical address through translation
    pub mem: FxHashMap<PAddr, Vec<u8>>,

    /// Page table that is used to translate virtual addresses into physical addresses and keep 
    /// track of mapped memory
    /// Address Decoding: [ L1:10 ][ L2:10 ][ offset:12 ]
    /// .0 - EXEC  Permission
    /// .1 - WRITE Permission
    /// .2 - READ  Permission
    pub page_table: Vec<Option<[PAddr; PAGE_TABLE_ENTRIES]>>,
    
    /// Memory loads will attempt to find data in caches first before resolving to retrieving them 
    /// from ram
    pub cache: Vec<CacheLine>,

    /// Least-recently-used queue that is used for cache-eviction algorithm
    pub lru_queue: VecDeque<u32>,

    /// Used to enable/disable caching
    pub cache_enabled: bool,
}

impl Default for Mmu {
    fn default() -> Self {
        Self::new()
    }
}

impl Mmu {
    /// Initialize a new default Mmu
    pub fn new() -> Self {
        Self {
            mem:            FxHashMap::default(),
            page_table:     vec![Option::None; PAGE_TABLE_ENTRIES],
            cache:          vec![CacheLine::default(); 32 * 4],
            lru_queue:      VecDeque::from([0, 1, 2, 3]),
            cache_enabled:  true,
        }
    }

    /// Completely flush cache
    pub fn clear_caches(&mut self) {
        self.cache = vec![CacheLine::default(); 32 * 4];
        self.lru_queue = VecDeque::from([0, 1, 2, 3]);
    }

    /// This performs a page-table walk to translate a given virtual address to a physical
    /// address
    pub fn translate_addr(&self, addr: VAddr, perms: u8) -> Result<PAddr, SimErr> {
        // Parse provided address to index page-table
        let idx_1  = ((addr.0 & 0xffc00000) >> 22) as usize;
        let idx_2  = ((addr.0 & 0x003ff000) >> 12) as usize;
        let offset =  addr.0 & (PAGE_SIZE as u32 - 1);

        if let Some(table_1) = &self.page_table[idx_1] {
            if (table_1[idx_2].0 & perms as u32) as u8 != perms {
                return Err(SimErr::Permission);
            }
            let page_base = table_1[idx_2].0 & !(PAGE_SIZE as u32 - 1);
            Ok(PAddr(page_base + offset))
        } else {
            Err(SimErr::AddrTranslation)
        }
    }

    /// Take a virtual address and create a page-table entry to map it to a physical entry
    pub fn map_page(&mut self, addr: VAddr, perms: u8) -> Result<(), SimErr> {
        let idx_1  = ((addr.0 & 0xffc00000) >> 22) as usize;
        let idx_2  = ((addr.0 & 0x003ff000) >> 12) as usize;

        if self.page_table[idx_1].is_none() {
            self.page_table[idx_1] = Some([PAddr(0u32); PAGE_TABLE_ENTRIES]);
        } 

        let table_2 = &mut self.page_table[idx_1].as_mut().unwrap();

        // Get a free-page from memory and increment paddr_base to indicate that this page is taken
        let mut rng = rand::thread_rng();

        // Find a random free page
        let mut new_page: PAddr;
        loop {
            let rand_num: u32 = rng.gen();
            new_page = PAddr(rand_num & !((1 << 12) - 1));
            assert_eq!(new_page.0 % PAGE_SIZE as u32, 0);
            if self.mem.get(&new_page).is_none() {
                self.mem.insert(new_page, vec![0u8; PAGE_SIZE]);
                break;
            }
        }

        // Encode permissions into stored address
        if table_2[idx_2] != PAddr(0) {
            return Err(SimErr::MemOverlap);
        }

        table_2[idx_2] = PAddr(new_page.0 | perms as u32);

        Ok(())
    }

    /// Load a page from ram
    pub fn mem_load_from_ram(&self, addr: PAddr, reader: &mut [u8]) -> Result<bool, SimErr> {
        let page_base = PAddr(addr.0 & !(PAGE_SIZE as u32 - 1));
        let offset    = (addr.0 & (PAGE_SIZE as u32 - 1)) as usize;

        let page = &self.mem.get(&page_base).ok_or(SimErr::AddrTranslation)?;

        reader.copy_from_slice(&page[offset..offset+reader.len()]);
        Ok(false)
    }

    /// Return `true` if `addr` is already cached and false if it is not and we need to hit 
    /// physical mem for it
    pub fn addr_in_cache(&self, addr: PAddr) -> bool {
        if !self.cache_enabled {
            return false;
        }

        let index  = (addr.0 & 0b11111000000) >> 6;
        let tag    = addr.0 >> 11;

        // Align address to 2^6 bounds to match the offset
        let cache_aligned_addr = PAddr(addr.0 & !((1 << 6) - 1));
        assert_eq!(cache_aligned_addr.0 % 64, 0);

        // 4-way associative, so lets loop through the 4 entries in this cache-set and see if we are
        // already in here, if so return true
        for i in 0..4 {
            let cacheline = &self.cache[((index * 4) + i) as usize];
            if tag == cacheline.tag as u32 && cacheline.is_valid {
                return true;
            }
        }

        // Requested memory was not found in cache so return false
        false
    }

    /// Takes a physical address `addr`, and loads `size` bytes
    /// 4-way set-associative
    /// 21 tag-bits,    
    /// 5 index-bits,  32 cache-set entries
    /// 6 offset-bits, 64 Bytes per line
    /// Returns true if cache-hit, false otherwise
    pub fn mem_load_from_cache(&mut self, addr: PAddr, reader: &mut [u8]) -> Result<bool, SimErr> {
        let offset = (addr.0 & 0b111111) as usize;
        let index  = (addr.0 & 0b11111000000) >> 6;
        let tag    = addr.0 >> 11;

        // Align address to 2^6 bounds to match the offset
        let cache_aligned_addr = PAddr(addr.0 & !((1 << 6) - 1));
        assert_eq!(cache_aligned_addr.0 % 64, 0);

        // 4-way associative, so lets loop through the 4 entries in this cache-set and see if we are
        // already in here, if so we can just read the data and return
        for i in 0..4 {
            let cacheline = &self.cache[((index * 4) + i) as usize];
            if tag == cacheline.tag as u32 && cacheline.is_valid {
                reader.copy_from_slice(&cacheline.data[offset..(reader.len() + offset)]);
                return Ok(true);
            }
        }

        // Loop through again and see if there exists an entry that isn't valid that we can just 
        // evict
        for i in 0..4 {
            if !&self.cache[((index * 4) + i) as usize].is_valid {
                // Load data from ram into this cache-set and mark it as valid
                let mut r1 = vec![0x0; 64];
                self.mem_load_from_ram(cache_aligned_addr, &mut r1)?;

                self.cache[((index * 4) + i) as usize].data = r1;
                self.cache[((index * 4) + i) as usize].tag = tag;
                self.cache[((index * 4) + i) as usize].is_valid = true;

                // Update LRU list by removing entry from middle and moving it to the back where it
                // will survive the longest before being marked for eviction
                for j in 0..self.lru_queue.len() {
                    if self.lru_queue[j] == i {
                        self.lru_queue.remove(j);
                        self.lru_queue.push_back(i);
                        break;
                    }
                }

                // Fill `reader` with bytes loaded into the cache from dram
                reader.copy_from_slice(&self.cache[((index * 4) + i) as usize]
                                       .data[offset..offset + reader.len()]);

                return Ok(false);
            }
        }

        // Evict from cache to insert

        // Get the entry at beginning of queue and move it to the end. We will be using this entry
        // for the cache-line so it should not be evicted anytime soon
        let lru = self.lru_queue.pop_front().unwrap();
        self.lru_queue.push_back(lru);

        // Populate entry
        let mut r1 = vec![0x0; 64];
        self.mem_load_from_ram(cache_aligned_addr, &mut r1)?;
        self.cache[((index * 4) + lru) as usize].data = r1;
        self.cache[((index * 4) + lru) as usize].tag = tag;
        self.cache[((index * 4) + lru) as usize].is_valid = true;

        reader.copy_from_slice(&self.cache[((index * 4) + lru) as usize]
                               .data[offset..offset + reader.len()]);

        return Ok(false);
    }

    /// Invalidate potential cache entry for `addr`
    pub fn mem_invalidate_cache(&mut self, addr: PAddr) -> Result<(), SimErr> {
        //let index  = (addr.0 & 0b11111) as usize;
        let index = (addr.0 & 0b11111000000) >> 6;
        let tag   = addr.0 >> 11;

        // Go through cache-sets for the index of this `addr` to see if there is an entry in the 
        // cache for this address. If there is, we invalidate it since we are now writing new data
        for i in 0..4 {
            let cacheline = &mut self.cache[((index * 4) + i) as usize];
            if tag == cacheline.tag as u32 && cacheline.is_valid {
                self.cache[((index * 4) + i) as usize].is_valid = false;
            }
        }
        Ok(())
    }

    /// Write `data` into memory at virtual address `addr`
    /// Currently we just invalidate caches for `addr` and write directly through to ram
    pub fn mem_write(&mut self, addr: VAddr, data: &[u8]) -> Result<(), SimErr> {
        let paddr     = self.translate_addr(addr, Perms::WRITE)?;
        let page_base = PAddr(paddr.0 & !(PAGE_SIZE as u32 - 1));
        let offset    = (paddr.0 & (PAGE_SIZE as u32 - 1)) as usize;

        // 32-bit architecture in which no instruction can write more than 4-bytes of memory at once
        assert!(data.len() <= 4, "Reads of more than 4-bytes at once are not supported");

        match data.len() {
            1 => {},
            2 => {
                // We only support 4-byte aligned accesses
                assert!((paddr.0 & 0x1) == 0, 
                        "2-byte reads need to be aligned on a 2-byte boundary. Provided address: \
                        {:x?}, is not", addr);
            },
            4 => {
                // We only support 4-byte aligned accesses
                assert!((paddr.0 & 0x3) == 0, 
                        "4-byte reads need to be aligned on a 4-byte boundary. Provided address: \
                        {:x?}, is not", addr);
            },
            _ => unreachable!(),
        }

        if self.cache_enabled {
            self.mem_invalidate_cache(paddr).unwrap();
        }

        // Write to memory
        let page = self.mem.get_mut(&page_base).unwrap();
        page[offset..(data.len() + offset)].copy_from_slice(data);

        Ok(())
    }

    /// Load `len` bytes from `addr` and return the bytes through the reader
    pub fn mem_read(&mut self, addr: VAddr, reader: &mut [u8]) -> Result<bool, SimErr> {
        let paddr = self.translate_addr(addr, Perms::READ)?;

        // 32-bit architecture in which no instruction can read more than 4-bytes of memory at once
        assert!(reader.len() <= 4, "Reads of more than 4-bytes at once are not supported");

        // We only support 4-byte aligned accesses
        assert!((paddr.0 & 0x3) == 0, 
                "Provided address: {:x?} is not aligned on 4-byte boundary", addr);

        if self.cache_enabled {
            self.mem_load_from_cache(paddr, reader)
        } else {
            self.mem_load_from_ram(paddr, reader)
        }
    }

    /// Load `len` bytes from `addr` and return the bytes through the reader
    /// Additional wrapper for gui to not mess up caches
    pub fn gui_mem_read(&mut self, addr: VAddr, reader: &mut [u8]) -> Result<bool, SimErr> {
        let paddr = self.translate_addr(addr, Perms::READ)?;

        // 32-bit architecture in which no instruction can read more than 4-bytes of memory at once
        assert!(reader.len() <= 4, "Reads of more than 4-bytes at once are not supported");

        // We only support 4-byte aligned accesses
        assert!((paddr.0 & 0x3) == 0, 
                "Provided address: {:x?} is not aligned on 4-byte boundary", addr);

        self.mem_load_from_ram(paddr, reader)
    }
}
