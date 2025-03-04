use super::{AccessType, IORegister, MemoryValue, CP15, HW};
use crate::hw::gpu::{Engine2D, EngineType, GPU};
use crate::num;

type MemoryRegion = ARM9MemoryRegion;

impl HW {
    const ITCM_MASK: u32 = HW::ITCM_SIZE as u32 - 1;
    const DTCM_MASK: u32 = HW::DTCM_SIZE as u32 - 1;

    pub fn arm9_read<T: MemoryValue>(&mut self, addr: u32) -> T {
        match MemoryRegion::from_addr(addr, &self.cp15) {
            MemoryRegion::ITCM => HW::read_mem(&self.itcm, addr & HW::ITCM_MASK),
            MemoryRegion::DTCM => HW::read_mem(&self.dtcm, addr & HW::DTCM_MASK),
            MemoryRegion::MainMem => HW::read_mem(&self.main_mem, addr & HW::MAIN_MEM_MASK),
            MemoryRegion::SharedWRAM if self.wramcnt.arm9_mask == 0 => {
                warn!("Reading from Unmapped ARM9 Shared WRAM: 0x{:X}", addr);
                num::zero()
            }
            MemoryRegion::SharedWRAM => HW::read_mem(
                &self.shared_wram,
                self.wramcnt.arm9_offset + (addr & self.wramcnt.arm9_mask),
            ),
            MemoryRegion::IO if (0x0410_0000..=0x0410_0003).contains(&addr) => {
                self.ipc_fifo_recv(true, addr)
            }
            MemoryRegion::IO if (0x0410_0010..=0x0410_0013).contains(&addr) => {
                self.read_game_card(true, addr)
            }
            MemoryRegion::IO => HW::read_from_bytes(self, &HW::arm9_read_io_register, addr),
            MemoryRegion::Palette if addr & 0x7FFF < 0x400 => {
                HW::read_from_bytes(&self.gpu.engine_a, &Engine2D::read_palette_ram, addr as u32)
            }
            MemoryRegion::Palette => {
                HW::read_from_bytes(&self.gpu.engine_b, &Engine2D::read_palette_ram, addr as u32)
            }
            MemoryRegion::VRAM => self.gpu.vram.arm9_read(addr),
            MemoryRegion::OAM if addr & 0x7FFF < 0x400 => {
                HW::read_mem(&self.gpu.engine_a.oam, addr & GPU::OAM_MASK as u32)
            }
            MemoryRegion::OAM => HW::read_mem(&self.gpu.engine_b.oam, addr & GPU::OAM_MASK as u32),
            MemoryRegion::GBAROM => self.read_gba_rom(true, addr),
            MemoryRegion::GBARAM => todo!(),
            MemoryRegion::BIOS => HW::read_mem(&self.bios9, addr & 0xFFFF),
            MemoryRegion::Unknown => {
                warn!("Reading from Unknown 0x{:08X}", addr);
                num::zero()
            }
        }
    }

    pub fn arm9_write<T: MemoryValue>(&mut self, addr: u32, value: T) {
        match MemoryRegion::from_addr(addr, &self.cp15) {
            MemoryRegion::ITCM => HW::write_mem(&mut self.itcm, addr & HW::ITCM_MASK, value),
            MemoryRegion::DTCM => HW::write_mem(&mut self.dtcm, addr & HW::DTCM_MASK, value),
            MemoryRegion::MainMem => {
                HW::write_mem(&mut self.main_mem, addr & HW::MAIN_MEM_MASK, value)
            }
            MemoryRegion::SharedWRAM if self.wramcnt.arm9_mask == 0 => {
                warn!("Writing to Unmapped ARM9 Shared WRAM")
            }
            MemoryRegion::SharedWRAM => HW::write_mem(
                &mut self.shared_wram,
                self.wramcnt.arm9_offset + addr & self.wramcnt.arm9_mask,
                value,
            ),
            MemoryRegion::IO if (0x0400_0188..=0x0400_018B).contains(&addr) => {
                self.ipc_fifo_send(false, addr, value)
            }
            MemoryRegion::IO if (0x0400_0400..0x0400_0440).contains(&addr) => {
                self.write_geometry_fifo(addr, value)
            }
            MemoryRegion::IO if (0x0400_0440..=0x0400_05CB).contains(&addr) => {
                self.write_geometry_command(addr, value)
            }
            MemoryRegion::IO => {
                HW::write_from_bytes(self, &HW::arm9_write_io_register, addr, value)
            }
            MemoryRegion::Palette if addr & 0x7FFF < 0x400 => {
                HW::write_palette_ram(&mut self.gpu.engine_a, addr, value)
            }
            MemoryRegion::Palette => HW::write_palette_ram(&mut self.gpu.engine_b, addr, value),
            MemoryRegion::VRAM => self.gpu.vram.arm9_write(addr, value),
            MemoryRegion::OAM if addr & 0x7FFF < 0x400 => HW::write_mem(
                &mut self.gpu.engine_a.oam,
                addr & GPU::OAM_MASK as u32,
                value,
            ),
            MemoryRegion::OAM => HW::write_mem(
                &mut self.gpu.engine_b.oam,
                addr & GPU::OAM_MASK as u32,
                value,
            ),
            MemoryRegion::GBAROM => (),
            MemoryRegion::GBARAM => todo!(),
            MemoryRegion::BIOS => warn!("Writing to BIOS9 0x{:08x} = 0x{:X}", addr, value),
            MemoryRegion::Unknown => warn!("Writing to Unknown 0x{:08X} = 0x{:X}", addr, value),
        }
    }

    pub fn arm9_get_access_time<T: MemoryValue>(
        &mut self,
        _access_type: AccessType,
        _addr: u32,
    ) -> usize {
        // TODO: Use accurate timings
        1
    }

    pub fn init_arm9(&mut self) -> u32 {
        let start_addr = self.cartridge.header().arm9_ram_addr;
        let rom_offset = self.cartridge.header().arm9_rom_offset as usize;
        let size = self.cartridge.header().arm9_size;
        for (i, addr) in (start_addr..start_addr + size).enumerate() {
            self.arm9_write(addr, self.cartridge.rom()[rom_offset + i]);
        }
        self.arm9_write(0x23FFC80, 0x5u8);
        self.cartridge.header().arm9_entry_addr
    }

    fn arm9_read_io_register(&self, addr: u32) -> u8 {
        match addr {
            0x0400_0000..=0x0400_0003 => self.gpu.engine_a.read_register(addr),
            0x0400_0004 => self.gpu.dispstats[1].read(0),
            0x0400_0005 => self.gpu.dispstats[1].read(1),
            0x0400_0006 => (self.gpu.vcount >> 0) as u8,
            0x0400_0007 => (self.gpu.vcount >> 8) as u8,
            0x0400_0008..=0x0400_005F => self.gpu.engine_a.read_register(addr),
            0x0400_0060..=0x0400_0063 => self.gpu.engine3d.disp3dcnt.read(addr as usize % 4),
            0x0400_0064..=0x0400_0067 => self.gpu.dispcapcnt.read(addr as usize % 4),
            0x0400_006C => self.gpu.engine_a.master_bright.read(0),
            0x0400_006D => self.gpu.engine_a.master_bright.read(1),
            0x0400_006E => self.gpu.engine_a.master_bright.read(2),
            0x0400_006F => self.gpu.engine_a.master_bright.read(3),
            0x0400_00B0..=0x0400_00BB => self.dmas[1].read(0, addr - 0xB0),
            0x0400_00BC..=0x0400_00C7 => self.dmas[1].read(1, addr - 0xBC),
            0x0400_00C8..=0x0400_00D3 => self.dmas[1].read(2, addr - 0xC8),
            0x0400_00D4..=0x0400_00DF => self.dmas[1].read(3, addr - 0xD4),
            0x0400_00E0..=0x0400_00E3 => {
                HW::read_byte_from_value(&self.dma_fill[0], addr as usize % 4)
            }
            0x0400_00E4..=0x0400_00E7 => {
                HW::read_byte_from_value(&self.dma_fill[1], addr as usize % 4)
            }
            0x0400_00E8..=0x0400_00EB => {
                HW::read_byte_from_value(&self.dma_fill[2], addr as usize % 4)
            }
            0x0400_00EC..=0x0400_00EF => {
                HW::read_byte_from_value(&self.dma_fill[3], addr as usize % 4)
            }
            0x0400_0100..=0x0400_0103 => self.timers[1][0].read(&self.scheduler, addr as usize % 4),
            0x0400_0104..=0x0400_0107 => self.timers[1][1].read(&self.scheduler, addr as usize % 4),
            0x0400_0108..=0x0400_010B => self.timers[1][2].read(&self.scheduler, addr as usize % 4),
            0x0400_010C..=0x0400_010F => self.timers[1][3].read(&self.scheduler, addr as usize % 4),
            0x0400_0130 => self.keypad.keyinput.read(0),
            0x0400_0131 => self.keypad.keyinput.read(1),
            0x0400_0132 => self.keypad.keycnt.read(0),
            0x0400_0133 => self.keypad.keycnt.read(1),
            0x0400_0136 => self.keypad.extkeyin.read(0),
            0x0400_0137 => self.keypad.extkeyin.read(1),
            0x0400_0180 => self.ipc.read_sync9(0),
            0x0400_0181 => self.ipc.read_sync9(1),
            0x0400_0182 => self.ipc.read_sync9(2),
            0x0400_0183 => self.ipc.read_sync9(3),
            0x0400_0184 => self.ipc.read_fifocnt9(0),
            0x0400_0185 => self.ipc.read_fifocnt9(1),
            0x0400_0186 => self.ipc.read_fifocnt9(2),
            0x0400_0187 => self.ipc.read_fifocnt9(3),
            0x0400_01A0 => self.cartridge.spicnt.read(!self.exmem.nds_arm7_access, 0),
            0x0400_01A1 => self.cartridge.spicnt.read(!self.exmem.nds_arm7_access, 1),
            0x0400_01A2 => self.cartridge.read_spi_data(!self.exmem.nds_arm7_access),
            0x0400_01A3 => 0, // Upper byte of AUXSPIDATA is always 0
            0x0400_01A4 => self.cartridge.read_romctrl(!self.exmem.nds_arm7_access, 0),
            0x0400_01A5 => self.cartridge.read_romctrl(!self.exmem.nds_arm7_access, 1),
            0x0400_01A6 => self.cartridge.read_romctrl(!self.exmem.nds_arm7_access, 2),
            0x0400_01A7 => self.cartridge.read_romctrl(!self.exmem.nds_arm7_access, 3),
            0x0400_0204 => self.exmem.read_arm9(),
            0x0400_0205 => self.exmem.read_common(),
            0x0400_0208 => self.interrupts[1].master_enable.read(0),
            0x0400_0209 => self.interrupts[1].master_enable.read(1),
            0x0400_020A => self.interrupts[1].master_enable.read(2),
            0x0400_020B => self.interrupts[1].master_enable.read(3),
            0x0400_0210 => self.interrupts[1].enable.read(0),
            0x0400_0211 => self.interrupts[1].enable.read(1),
            0x0400_0212 => self.interrupts[1].enable.read(2),
            0x0400_0213 => self.interrupts[1].enable.read(3),
            0x0400_0214 => self.interrupts[1].request.read(0),
            0x0400_0215 => self.interrupts[1].request.read(1),
            0x0400_0216 => self.interrupts[1].request.read(2),
            0x0400_0217 => self.interrupts[1].request.read(3),
            0x0400_0240..=0x0400_0246 => self.gpu.vram.read_vram_cnt(addr as usize & 0xF),
            0x0400_0247 => self.wramcnt.read(0),
            0x0400_0248..=0x0400_0249 => self.gpu.vram.read_vram_cnt((addr as usize & 0xF) - 1),
            0x0400_0280..=0x0400_0283 => self.div.cnt.read(addr as usize & 0xF),
            0x0400_0290..=0x0400_0297 => self.div.read_numer(addr as usize & 0x7),
            0x0400_0298..=0x0400_029F => self.div.read_denom(addr as usize & 0x7),
            0x0400_02A0..=0x0400_02A7 => self.div.read_quot(addr as usize & 0x7),
            0x0400_02A8..=0x0400_02AF => self.div.read_rem(addr as usize & 0x7),
            0x0400_02B0..=0x0400_02B3 => self.sqrt.cnt.read(addr as usize & 0xF),
            0x0400_02B4..=0x0400_02B7 => self.sqrt.read_result(addr as usize & 0x3),
            0x0400_02B8..=0x0400_02BF => self.sqrt.read_param(addr as usize & 0x7),
            0x0400_0300 => self.postflg9,
            0x0400_0301..=0x0400_0303 => 0, // Other Parts of POSTFLG
            0x0400_0304 => self.gpu.powcnt1.read(0),
            0x0400_0305 => self.gpu.powcnt1.read(1),
            0x0400_0306 => self.gpu.powcnt1.read(2),
            0x0400_0307 => self.gpu.powcnt1.read(3),
            0x0400_0320..=0x0400_06A3 => self.gpu.engine3d.read_register(addr),
            0x0400_1000..=0x0400_1003 => self.gpu.engine_b.read_register(addr),
            0x0400_1004..=0x0400_1007 => 0,
            0x0400_1008..=0x0400_105F => self.gpu.engine_b.read_register(addr),
            0x0400_1060..=0x0400_106B => 0,
            0x0400_106C => self.gpu.engine_b.master_bright.read(0),
            0x0400_106D => self.gpu.engine_b.master_bright.read(1),
            0x0400_106E => self.gpu.engine_b.master_bright.read(2),
            0x0400_106F => self.gpu.engine_b.master_bright.read(3),
            0x0400_4010..=0x0400_4011 => 0, // DSi register that's unused for NDS
            _ => {
                warn!("Ignoring ARM9 IO Register Read at 0x{:08X}", addr);
                0
            }
        }
    }

    fn arm9_write_io_register(&mut self, addr: u32, value: u8) {
        match addr {
            0x0400_0000..=0x0400_0003 => {
                self.gpu
                    .engine_a
                    .write_register(&mut self.scheduler, addr, value)
            }
            0x0400_0004 => self.gpu.dispstats[1].write(&mut self.scheduler, 0, value),
            0x0400_0005 => self.gpu.dispstats[1].write(&mut self.scheduler, 1, value),
            0x0400_0006 => (), // VCOUNT is read only
            0x0400_0007 => (), // VCOUNT is read only
            0x0400_0008..=0x0400_005F => {
                self.gpu
                    .engine_a
                    .write_register(&mut self.scheduler, addr, value)
            }
            0x0400_0060..=0x0400_0063 => {
                self.gpu
                    .engine3d
                    .disp3dcnt
                    .write(&mut self.scheduler, addr as usize % 4, value)
            }
            0x0400_0064..=0x0400_0067 => {
                self.gpu
                    .dispcapcnt
                    .write(&mut self.scheduler, addr as usize % 4, value)
            }
            0x0400_006C => self
                .gpu
                .engine_a
                .master_bright
                .write(&mut self.scheduler, 0, value),
            0x0400_006D => self
                .gpu
                .engine_a
                .master_bright
                .write(&mut self.scheduler, 1, value),
            0x0400_006E => self
                .gpu
                .engine_a
                .master_bright
                .write(&mut self.scheduler, 2, value),
            0x0400_006F => self
                .gpu
                .engine_a
                .master_bright
                .write(&mut self.scheduler, 3, value),
            0x0400_00B0..=0x0400_00BB => {
                self.dmas[1].write(0, &mut self.scheduler, addr - 0xB0, value)
            }
            0x0400_00BC..=0x0400_00C7 => {
                self.dmas[1].write(1, &mut self.scheduler, addr - 0xBC, value)
            }
            0x0400_00C8..=0x0400_00D3 => {
                self.dmas[1].write(2, &mut self.scheduler, addr - 0xC8, value)
            }
            0x0400_00D4..=0x0400_00DF => {
                self.dmas[1].write(3, &mut self.scheduler, addr - 0xD4, value)
            }
            0x0400_00E0..=0x0400_00E3 => {
                HW::write_byte_to_value(&mut self.dma_fill[0], addr as usize % 4, value)
            }
            0x0400_00E4..=0x0400_00E7 => {
                HW::write_byte_to_value(&mut self.dma_fill[1], addr as usize % 4, value)
            }
            0x0400_00E8..=0x0400_00EB => {
                HW::write_byte_to_value(&mut self.dma_fill[2], addr as usize % 4, value)
            }
            0x0400_00EC..=0x0400_00EF => {
                HW::write_byte_to_value(&mut self.dma_fill[3], addr as usize % 4, value)
            }
            0x0400_0100..=0x0400_0103 => {
                self.timers[1][0].write(&mut self.scheduler, addr as usize % 4, value)
            }
            0x0400_0104..=0x0400_0107 => {
                self.timers[1][1].write(&mut self.scheduler, addr as usize % 4, value)
            }
            0x0400_0108..=0x0400_010B => {
                self.timers[1][2].write(&mut self.scheduler, addr as usize % 4, value)
            }
            0x0400_010C..=0x0400_010F => {
                self.timers[1][3].write(&mut self.scheduler, addr as usize % 4, value)
            }
            0x0400_0130 => self.keypad.keyinput.write(&mut self.scheduler, 0, value),
            0x0400_0131 => self.keypad.keyinput.write(&mut self.scheduler, 1, value),
            0x0400_0132 => self.keypad.keycnt.write(&mut self.scheduler, 0, value),
            0x0400_0133 => self.keypad.keycnt.write(&mut self.scheduler, 1, value),
            0x0400_0136 => self.keypad.extkeyin.write(&mut self.scheduler, 0, value),
            0x0400_0137 => self.keypad.extkeyin.write(&mut self.scheduler, 1, value),
            0x0400_0180 => self.interrupts[0].request |= self.ipc.write_sync9(0, value),
            0x0400_0181 => self.interrupts[0].request |= self.ipc.write_sync9(1, value),
            0x0400_0182 => self.interrupts[0].request |= self.ipc.write_sync9(2, value),
            0x0400_0183 => self.interrupts[0].request |= self.ipc.write_sync9(3, value),
            0x0400_0184 => self.interrupts[1].request |= self.ipc.write_fifocnt9(0, value),
            0x0400_0185 => self.interrupts[1].request |= self.ipc.write_fifocnt9(1, value),
            0x0400_0186 => self.interrupts[1].request |= self.ipc.write_fifocnt9(2, value),
            0x0400_0187 => self.interrupts[1].request |= self.ipc.write_fifocnt9(3, value),
            0x0400_01A0 => self
                .cartridge
                .spicnt
                .write(!self.exmem.nds_arm7_access, 0, value),
            0x0400_01A1 => self
                .cartridge
                .spicnt
                .write(!self.exmem.nds_arm7_access, 1, value),
            0x0400_01A2 => self
                .cartridge
                .write_spi_data(!self.exmem.nds_arm7_access, value),
            0x0400_01A3 => (), // TODO: Does this write do anything?
            0x0400_01A4 => self.cartridge.write_romctrl(
                &mut self.scheduler,
                false,
                !self.exmem.nds_arm7_access,
                0,
                value,
            ),
            0x0400_01A5 => self.cartridge.write_romctrl(
                &mut self.scheduler,
                false,
                !self.exmem.nds_arm7_access,
                1,
                value,
            ),
            0x0400_01A6 => self.cartridge.write_romctrl(
                &mut self.scheduler,
                false,
                !self.exmem.nds_arm7_access,
                2,
                value,
            ),
            0x0400_01A7 => self.cartridge.write_romctrl(
                &mut self.scheduler,
                false,
                !self.exmem.nds_arm7_access,
                3,
                value,
            ),
            0x0400_01A8 => self
                .cartridge
                .write_command(!self.exmem.nds_arm7_access, 0, value),
            0x0400_01A9 => self
                .cartridge
                .write_command(!self.exmem.nds_arm7_access, 1, value),
            0x0400_01AA => self
                .cartridge
                .write_command(!self.exmem.nds_arm7_access, 2, value),
            0x0400_01AB => self
                .cartridge
                .write_command(!self.exmem.nds_arm7_access, 3, value),
            0x0400_01AC => self
                .cartridge
                .write_command(!self.exmem.nds_arm7_access, 4, value),
            0x0400_01AD => self
                .cartridge
                .write_command(!self.exmem.nds_arm7_access, 5, value),
            0x0400_01AE => self
                .cartridge
                .write_command(!self.exmem.nds_arm7_access, 6, value),
            0x0400_01AF => self
                .cartridge
                .write_command(!self.exmem.nds_arm7_access, 7, value),
            0x0400_0204 => self.exmem.write_arm9(value),
            0x0400_0205 => self.exmem.write_common(value),
            0x0400_0208 => self.interrupts[1]
                .master_enable
                .write(&mut self.scheduler, 0, value),
            0x0400_0209 => self.interrupts[1]
                .master_enable
                .write(&mut self.scheduler, 1, value),
            0x0400_020A => self.interrupts[1]
                .master_enable
                .write(&mut self.scheduler, 2, value),
            0x0400_020B => self.interrupts[1]
                .master_enable
                .write(&mut self.scheduler, 3, value),
            0x0400_0210 => self.interrupts[1]
                .enable
                .write(&mut self.scheduler, 0, value),
            0x0400_0211 => self.interrupts[1]
                .enable
                .write(&mut self.scheduler, 1, value),
            0x0400_0212 => self.interrupts[1]
                .enable
                .write(&mut self.scheduler, 2, value),
            0x0400_0213 => self.interrupts[1]
                .enable
                .write(&mut self.scheduler, 3, value),
            0x0400_0214 => self.interrupts[1]
                .request
                .write(&mut self.scheduler, 0, value),
            0x0400_0215 => self.interrupts[1]
                .request
                .write(&mut self.scheduler, 1, value),
            0x0400_0216 => self.interrupts[1]
                .request
                .write(&mut self.scheduler, 2, value),
            0x0400_0217 => self.interrupts[1]
                .request
                .write(&mut self.scheduler, 3, value),
            0x0400_0240..=0x0400_0246 => self.gpu.vram.write_vram_cnt(addr as usize & 0xF, value),
            0x0400_0247 => self.wramcnt.write(&mut self.scheduler, 0, value),
            0x0400_0248..=0x0400_0249 => self
                .gpu
                .vram
                .write_vram_cnt((addr as usize & 0xF) - 1, value),
            0x0400_0280..=0x0400_0283 => {
                self.div
                    .cnt
                    .write(&mut self.scheduler, addr as usize & 0xF, value)
            }
            0x0400_0290..=0x0400_0297 => {
                self.div
                    .write_numer(&mut self.scheduler, addr as usize & 0x7, value)
            }
            0x0400_0298..=0x0400_029F => {
                self.div
                    .write_denom(&mut self.scheduler, addr as usize & 0x7, value)
            }
            0x0400_02A0..=0x0400_02A7 => (), // Div result registers are read-only
            0x0400_02A8..=0x0400_02AF => (), // Div result registers are read-only
            0x0400_02B0..=0x0400_02B3 => {
                self.sqrt
                    .cnt
                    .write(&mut self.scheduler, addr as usize & 0xF, value)
            }
            0x0400_02B4..=0x0400_02B7 => (), // Sqrt result register is read-only
            0x0400_02B8..=0x0400_02BF => {
                self.sqrt
                    .write_param(&mut self.scheduler, addr as usize & 0x7, value)
            }
            0x0400_0300 => self.postflg9 = (self.postflg9 & !0x02 | value & 0x02) | (value & 0x1), // Only bit 1 is writable
            0x0400_0301..=0x0400_0303 => (), // Other Parts of POSTFLG
            0x0400_0304 => self.gpu.powcnt1.write(&mut self.scheduler, 0, value),
            0x0400_0305 => self.gpu.powcnt1.write(&mut self.scheduler, 1, value),
            0x0400_0306 => self.gpu.powcnt1.write(&mut self.scheduler, 2, value),
            0x0400_0307 => self.gpu.powcnt1.write(&mut self.scheduler, 3, value),
            0x0400_0320..=0x0400_06A3 => {
                self.gpu
                    .engine3d
                    .write_register(&mut self.scheduler, addr, value)
            }
            0x0400_1000..=0x0400_1003 => {
                self.gpu
                    .engine_b
                    .write_register(&mut self.scheduler, addr, value)
            }
            0x0400_1004..=0x0400_1007 => (),
            0x0400_1008..=0x0400_105F => {
                self.gpu
                    .engine_b
                    .write_register(&mut self.scheduler, addr, value)
            }
            0x0400_1060..=0x0400_106B => (),
            0x0400_106C => self
                .gpu
                .engine_b
                .master_bright
                .write(&mut self.scheduler, 0, value),
            0x0400_106D => self
                .gpu
                .engine_b
                .master_bright
                .write(&mut self.scheduler, 1, value),
            0x0400_106E => self
                .gpu
                .engine_b
                .master_bright
                .write(&mut self.scheduler, 2, value),
            0x0400_106F => self
                .gpu
                .engine_b
                .master_bright
                .write(&mut self.scheduler, 3, value),
            _ => warn!(
                "Ignoring ARM9 IO Register Write 0x{:08X} = {:02X}",
                addr, value
            ),
        }
    }

    fn write_geometry_fifo<T: MemoryValue>(&mut self, addr: u32, value: T) {
        assert!(addr % 4 == 0 && std::mem::size_of::<T>() == 4);
        self.gpu
            .engine3d
            .write_geometry_fifo(num::cast::<T, u32>(value).unwrap());
    }

    fn write_geometry_command<T: MemoryValue>(&mut self, addr: u32, value: T) {
        assert!(addr % 4 == 0 && std::mem::size_of::<T>() == 4);
        self.gpu
            .engine3d
            .write_geometry_command(addr, num::cast::<T, u32>(value).unwrap());
        self.check_geometry_command_fifo();
    }

    fn write_palette_ram<E: EngineType, T: MemoryValue>(
        engine: &mut Engine2D<E>,
        addr: u32,
        value: T,
    ) {
        let addr = addr as usize;
        match std::mem::size_of::<T>() {
            1 => (), // Ignore byte writes
            2 => engine.write_palette_ram(addr, num::cast::<T, u16>(value).unwrap()),
            4 => {
                let value = num::cast::<T, u32>(value).unwrap();
                engine.write_palette_ram(addr, value as u16);
                engine.write_palette_ram(addr + 2, (value >> 16) as u16);
            }
            _ => unreachable!(),
        }
    }
}

#[derive(PartialEq)]
pub enum ARM9MemoryRegion {
    ITCM,
    DTCM,
    MainMem,
    SharedWRAM,
    IO,
    Palette,
    VRAM,
    OAM,
    GBAROM,
    GBARAM,
    BIOS,
    Unknown,
}

impl ARM9MemoryRegion {
    pub fn from_addr(addr: u32, cp15: &CP15) -> Self {
        use ARM9MemoryRegion::*;
        if cp15.addr_in_itcm(addr) {
            return ITCM;
        }
        if cp15.addr_in_dtcm(addr) {
            return DTCM;
        }
        match addr >> 24 {
            0x2 => MainMem,
            0x3 => SharedWRAM,
            0x4 => IO,
            0x5 => Palette,
            0x6 => VRAM,
            0x7 => OAM,
            0x8 | 0x9 => GBAROM,
            0xA => GBARAM,
            0xFF if addr >> 16 == 0xFFFF => BIOS,
            _ => {
                warn!("Uknown Memory Access: {:X}", addr);
                Unknown
            }
        }
    }
}
