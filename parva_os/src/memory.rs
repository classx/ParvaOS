use bootloader::bootinfo::{BootInfo, MemoryMap, MemoryRegionType};
use x86_64::structures::paging::mapper::MapperAllSizes;
use x86_64::structures::paging::{FrameAllocator, OffsetPageTable, PageTable, PhysFrame, Size4KiB};
use x86_64::{PhysAddr, VirtAddr};
use crate::print;

// NOTE: This static is mutable but it'll be changed only once during initialization
static mut PHYS_MEM_OFFSET: u64 = 0;

pub fn init(boot_info: &'static BootInfo) {
    let mut memory_size = 0;
    for region in boot_info.memory_map.iter() {
        let start_addr = region.range.start_addr();
        let end_addr = region.range.end_addr();
        memory_size += end_addr - start_addr;
        print!("MEM [{:#016X}-{:#016X}] {:?}\n", start_addr, end_addr, region.region_type);
    }
    print!("MEM {} KB\n", memory_size >> 10);

    unsafe { PHYS_MEM_OFFSET = boot_info.physical_memory_offset; }

    let mut mapper = unsafe { mapper(VirtAddr::new(PHYS_MEM_OFFSET)) };
    let mut frame_allocator = unsafe { BootInfoFrameAllocator::init(&boot_info.memory_map) };
    crate::allocator::init_heap(&mut mapper, &mut frame_allocator).expect("heap initialization failed");
}

pub fn phys_to_virt(addr: PhysAddr) -> VirtAddr {
    VirtAddr::new(addr.as_u64() + unsafe { PHYS_MEM_OFFSET })
}

pub fn virt_to_phys(addr: VirtAddr) -> Option<PhysAddr> {
    let mapper = unsafe { mapper(VirtAddr::new(PHYS_MEM_OFFSET)) };
    mapper.translate_addr(addr)
}

pub unsafe fn mapper(physical_memory_offset: VirtAddr) -> OffsetPageTable<'static> {
    let level_4_table = active_level_4_table(physical_memory_offset);
    OffsetPageTable::new(level_4_table, physical_memory_offset)
}

unsafe fn active_level_4_table(physical_memory_offset: VirtAddr) -> &'static mut PageTable {
    use x86_64::registers::control::Cr3;

    let (level_4_table_frame, _) = Cr3::read();

    let phys = level_4_table_frame.start_address();
    let virt = physical_memory_offset + phys.as_u64();
    let page_table_ptr: *mut PageTable = virt.as_mut_ptr();

    &mut *page_table_ptr // unsafe
}

pub struct BootInfoFrameAllocator {
    memory_map: &'static MemoryMap,
    next: usize,
}

impl BootInfoFrameAllocator {
    pub unsafe fn init(memory_map: &'static MemoryMap) -> Self {
        BootInfoFrameAllocator { memory_map, next: 0 }
    }

    fn usable_frames(&self) -> impl Iterator<Item = PhysFrame> {
        let regions = self.memory_map.iter();
        let usable_regions = regions.filter(|r| r.region_type == MemoryRegionType::Usable);
        let addr_ranges = usable_regions.map(|r| r.range.start_addr()..r.range.end_addr());
        let frame_addresses = addr_ranges.flat_map(|r| r.step_by(4096));
        frame_addresses.map(|addr| PhysFrame::containing_address(PhysAddr::new(addr)))
    }
}

unsafe impl FrameAllocator<Size4KiB> for BootInfoFrameAllocator {
    fn allocate_frame(&mut self) -> Option<PhysFrame> {
        let frame = self.usable_frames().nth(self.next);
        self.next += 1;
        frame
    }
}
