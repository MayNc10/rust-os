use crate::println;

use super::*;

use lazy_static::lazy_static;
use spin::Mutex;
use x86_64::instructions::port::{Port, PortGeneric, ReadWriteAccess};

pub static WRITE_COMMAND: u8 = 0x30;
pub static READ_COMMAND: u8 = 0x20;

#[repr(u8)]
pub enum IOPortRead {
    DataRegister = 0,
    ErrorRegister,
    SectorCountRegister,
    LBALow,
    LBAMid,
    LBAHigh,
    DriveSelectRegister,
    StatusRegister,
}

#[repr(u8)]
pub enum IOPortWrite {
    DataRegister = 0,
    FeaturesRegister,
    SectorCountRegister,
    SectorNumberRegister,
    CylinderLowRegister,
    CylinderHighRegister,
    DriveAndHeadRegister,
    CommandRegister,
}

#[repr(u8)]
pub enum ControlPortRead {
    AlternateStatusRegister = 0,
    DriveAddressRegister,
}

#[repr(u8)]
pub enum ControlPortWrite {
    DeviceControlRegister,
}

pub mod status {
    #[repr(u8)]
    pub enum Bitflags {
        ERR = 0,
        IDX,
        CORR,
        DRQ,
        SRV,
        DF,
        RDY,
        BSY,
    }

    #[derive(Clone, Copy)]
    pub struct Status {
        pub val: u8,
    }
    impl Status {
        pub fn error(&self) -> bool { self.val & (1 << Bitflags::ERR as u8) > 0 }
        pub fn drive_request(&self) -> bool { self.val & (1 << Bitflags::DRQ as u8) > 0 }
        pub fn service_request(&self) -> bool { self.val & (1 << Bitflags::SRV as u8) > 0 }
        pub fn drive_fault(&self) -> bool { self.val & (1 << Bitflags::DF as u8) > 0 }
        pub fn ready(&self) -> bool { self.val & (1 << Bitflags::RDY as u8) > 0 }
        pub fn busy(&self) -> bool { self.val & (1 << Bitflags::BSY as u8) > 0 }
    }
}

pub mod error {
    #[repr(u8)]
    pub enum Bitflags {
        AMNF = 0,
        TKZNF,
        ABRT,
        MCR,
        IDNF,
        MC,
        UNC,
        BBK,
    }

    #[derive(Clone, Copy)]
    pub struct Error {
        pub val: u8,
    }
    impl Error {
        pub fn address_mark_not_found(&self) -> bool { self.val & (1 << Bitflags::AMNF as u8) > 0 }
        pub fn track_zero_not_found(&self) -> bool { self.val & (1 << Bitflags::TKZNF as u8) > 0 }
        pub fn aborted_command(&self) -> bool { self.val & (1 << Bitflags::ABRT as u8) > 0 }
        pub fn media_change_request(&self) -> bool { self.val & (1 << Bitflags::MCR as u8) > 0 }
        pub fn id_not_found(&self) -> bool { self.val & (1 << Bitflags::IDNF as u8) > 0 }
        pub fn media_changed(&self) -> bool { self.val & (1 << Bitflags::MC as u8) > 0 }
        pub fn uncorrectable_data(&self) -> bool { self.val & (1 << Bitflags::UNC as u8) > 0 }
        pub fn bad_block(&self) -> bool { self.val & (1 << Bitflags::BBK as u8) > 0 }
    }
}

type Disk = u8;

#[repr(u8)]
#[derive(Clone, Copy)]
pub enum Bus {
    Primary = 0,
    Secondary,
}


pub static BUS_IO_BASES: [u16; 2] = [ATA_IO_PORT_PRIMARY, ATA_IO_PORT_SECONDARY];
pub static BUS_CONTROL_BASES: [u16; 2] = [ATA_CONTROL_PORT_PRIMARY, ATA_CONTROL_PORT_SECONDARY];

pub struct Driver {
    status: status::Status,
    disk: Disk,
    bus: Bus,
}

impl Driver {
    pub fn new() -> Driver {
        let disk = 0;
        let bus = Bus::Primary;
        let mut p = Port::new(BUS_IO_BASES[bus as u8 as usize] + IOPortRead::StatusRegister as u16);
        let status = status::Status { val: unsafe { p.read() } };
        Driver { status, disk, bus}
    }
    pub fn wait_bsy(&mut self) {
        self.read_status();
        while self.status.busy() {
            self.read_status();
        }
    }
    pub fn wait_drq(&mut self) {
        self.read_status();
        while !self.status.drive_request() {
            //println!("{}", self.status.val);
            self.read_status();
        }
    }
    pub fn wait_rdy(&mut self) {
        self.read_status();
        while !self.status.ready() {
            println!("{}", self.status.val);
            self.read_status();
        }
    }
    pub fn read(&mut self, buf: &mut [u16], lba: u32, sector_count: u8) {
        self.wait_bsy();
        let mut dsel_reg = Port::new(BUS_IO_BASES[self.bus as u8 as usize] + IOPortRead::DriveSelectRegister as u16);
        let mut sec_count_reg = Port::new(BUS_IO_BASES[self.bus as u8 as usize] + IOPortRead::SectorCountRegister as u16);
        let mut lba_lo_reg = Port::new(BUS_IO_BASES[self.bus as u8 as usize] + IOPortRead::LBALow as u16);
        let mut lba_mid_reg = Port::new(BUS_IO_BASES[self.bus as u8 as usize] + IOPortRead::LBAMid as u16);
        let mut lba_high_reg = Port::new(BUS_IO_BASES[self.bus as u8 as usize] + IOPortRead::LBAHigh as u16);
        let mut cmd_reg = Port::new(BUS_IO_BASES[self.bus as u8 as usize] + IOPortWrite::CommandRegister as u16);
        let mut data_reg = Port::new(BUS_IO_BASES[self.bus as u8 as usize] + IOPortRead::DataRegister as u16);

        unsafe {
            let top_byte = (lba >> 24) & 0xF;
            dsel_reg.write({self.disk << 4} | top_byte as u8 | (0x1 << 6)); 
            sec_count_reg.write(sector_count);
            lba_lo_reg.write((lba & 0xFF) as u8);
            lba_mid_reg.write(((lba >> 8) & 0xFF) as u8);
            lba_high_reg.write(((lba >> 16) & 0xFF) as u8);
            cmd_reg.write(READ_COMMAND);

            for sec in 0..sector_count as usize {
                self.wait_bsy();
                self.wait_drq();
                for word in 0..256 {
                    buf[sec * 256 + word as usize] = data_reg.read();
                }
            }
        }   
    }
    pub fn write(&mut self, data: &mut [u16], lba: u32, sector_count: u8) {
        self.wait_bsy();
        let mut dsel_reg = Port::new(BUS_IO_BASES[self.bus as u8 as usize] + IOPortRead::DriveSelectRegister as u16);
        let mut sec_count_reg = Port::new(BUS_IO_BASES[self.bus as u8 as usize] + IOPortRead::SectorCountRegister as u16);
        let mut lba_lo_reg = Port::new(BUS_IO_BASES[self.bus as u8 as usize] + IOPortRead::LBALow as u16);
        let mut lba_mid_reg = Port::new(BUS_IO_BASES[self.bus as u8 as usize] + IOPortRead::LBAMid as u16);
        let mut lba_high_reg = Port::new(BUS_IO_BASES[self.bus as u8 as usize] + IOPortRead::LBAHigh as u16);
        let mut cmd_reg = Port::new(BUS_IO_BASES[self.bus as u8 as usize] + IOPortWrite::CommandRegister as u16);
        let mut data_reg = Port::new(BUS_IO_BASES[self.bus as u8 as usize] + IOPortRead::DataRegister as u16);

        unsafe {
            let top_byte = (lba >> 24) & 0xF;
            dsel_reg.write({self.disk << 4} | top_byte as u8 | (0x1 << 6)); 
            sec_count_reg.write(sector_count);
            lba_lo_reg.write((lba & 0xFF) as u8);
            lba_mid_reg.write((lba >> 8 & 0xFF) as u8);
            lba_high_reg.write((lba >> 16 & 0xFF) as u8);
            cmd_reg.write(WRITE_COMMAND);

            for sec in 0..sector_count as usize {
                self.wait_bsy();
                self.wait_drq();
                for word in 0..256 {
                    data_reg.write(data[sec * 256 + word as usize]);
                }
            }
        }   
    }
    pub fn status(&self) -> status::Status { self.status }
    pub fn read_status(&mut self) {
        let mut p = Port::new(BUS_IO_BASES[self.bus as u8 as usize] + IOPortRead::StatusRegister as u16);
        self.status = status::Status { val: unsafe { p.read() } };
    }
    /* 
    pub fn identify_device(&mut self) -> [u8; 512] {
        self.wait_bsy();
        self.wait_rdy();
        let mut dh_reg: PortGeneric<u8, ReadWriteAccess> = Port::new(BUS_IO_BASES[self.bus as u8 as usize] + IOPortRead::DriveSelectRegister as u16);
        //let mut sec_count_reg = Port::new(DISK_IO_BASES[self.disk as u8 as usize] + IOPortRead::SectorCountRegister as u16);
        //let mut lba_lo_reg = Port::new(DISK_IO_BASES[self.disk as u8 as usize] + IOPortRead::SectorNumberRegister as u16);
        //let mut lba_mid_reg = Port::new(DISK_IO_BASES[self.disk as u8 as usize] + IOPortRead::CylinderLowRegister as u16);
        //let mut lba_high_reg = Port::new(DISK_IO_BASES[self.disk as u8 as usize] + IOPortRead::CylinderHighRegister as u16);
        let mut data_reg = Port::new(BUS_IO_BASES[self.bus as u8 as usize] + IOPortRead::DataRegister as u16);
        let mut cmd_reg = Port::new(BUS_IO_BASES[self.bus as u8 as usize] + IOPortWrite::CommandRegister as u16);
        let buf = [0_u8; 512];

        unsafe {
            cmd_reg.write(0xEC_u8); // should be ECh
            let mut buf = [0_u8; 512];
            self.wait_bsy();
            self.wait_rdy();
            for word in 0..512 {
                buf[word] = data_reg.read();
            }
        }
        buf
    }
    */
    pub fn identify(&mut self) -> [u16; 256] {
        println!("Identifying device");

        let mut dh_reg = Port::new(BUS_IO_BASES[self.bus as u8 as usize] + IOPortRead::DriveSelectRegister as u16);
        let mut sec_count_reg = Port::new(BUS_IO_BASES[self.bus as u8 as usize] + IOPortRead::SectorCountRegister as u16);
        let mut lba_lo_reg = Port::new(BUS_IO_BASES[self.bus as u8 as usize] + IOPortRead::LBALow as u16);
        let mut lba_mid_reg = Port::new(BUS_IO_BASES[self.bus as u8 as usize] + IOPortRead::LBAMid as u16);
        let mut lba_high_reg = Port::new(BUS_IO_BASES[self.bus as u8 as usize] + IOPortRead::LBAHigh as u16);
        let mut cmd_reg = Port::new(BUS_IO_BASES[self.bus as u8 as usize] + IOPortWrite::CommandRegister as u16);
        let mut data_reg = Port::new(BUS_IO_BASES[self.bus as u8 as usize] + IOPortRead::DataRegister as u16);

        let mut data = [0; 256];
        unsafe {
            println!("Writing drive selection and port zeros");
            dh_reg.write(0xA0_u8 | (self.disk << 4) ); //if is_master_drive { 0xA0_u8 } else { 0xB0_u8 }
            sec_count_reg.write(0x0_u8);
            lba_lo_reg.write(0x0_u8);
            lba_mid_reg.write(0x0_u8);
            lba_high_reg.write(0x0_u8);
            
            println!("Written those, writing command");
            cmd_reg.write(0xEC_u8);
            self.read_status();
            if self.status.val == 0 {
                println!("No drive found");
            }
            else {
                println!("Written command, waiting");
                self.wait_bsy();
                println!("Busy signal low, waiting for drive ready");
                self.wait_drq();
                println!("Collecting data");
                for i in 0..256 {
                    data[i] = data_reg.read();
                }
            }
        }
        println!("Exiting...");
        return data;
    }
    pub fn drive_selected(&self) -> Option<Disk> {
        let mut da_reg = Port::new(BUS_CONTROL_BASES[self.bus as u8 as usize] + 1);
        unsafe {
            let drive_addr: u8 = da_reg.read();
            if drive_addr & 0x1 == drive_addr & 0x2 { None }
            else { Some(drive_addr & 0x3) }
        }
    }
    pub fn change_disk(&mut self, disk: Disk) {
        self.disk = disk;
        //let mut dsel_reg = Port::new(BUS_IO_BASES[self.bus as u8 as usize] + IOPortRead::DriveSelectRegister as u16);
        unsafe {
            //dsel_reg.write(0xA0 | (disk << 4))
        }
        self.read_status();
    }
    pub fn current_disk(&self) -> Disk {
        let mut daddr_reg = Port::new(BUS_CONTROL_BASES[self.bus as u8 as usize] 
            + ControlPortRead::DriveAddressRegister as u16);
        unsafe {
            let addr: u8 = daddr_reg.read();
            if addr & 1 == 0 { return 0; }
            else if addr & 2 == 0 { return 1; }
            else { panic!("Illegal drive address: {}", addr) }
        }
    }
    pub fn change_bus(&mut self, bus: Bus) {
        self.bus = bus;
    }
}

lazy_static! {
    pub static ref DRIVER: Mutex<Driver> = Mutex::new(Driver::new());
}

