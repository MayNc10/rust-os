use crate::println;

use super::*;

use lazy_static::lazy_static;
use spin::Mutex;
use x86_64::instructions::port::Port;

pub static WRITE_COMMAND: u8 = 0x30;
pub static READ_COMMAND: u8 = 0x20;

#[repr(u8)]
pub enum IOPortRead {
    DataRegister = 0,
    ErrorRegister,
    SectorCountRegister,
    SectorNumberRegister,
    CylinderLowRegister,
    CylinderHighRegister,
    DriveAndHeadRegister,
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
        pub fn error(&self) -> bool { self.val & Bitflags::ERR as u8 > 0 }
        pub fn drive_request(&self) -> bool { self.val & Bitflags::DRQ as u8 > 0 }
        pub fn service_request(&self) -> bool { self.val & Bitflags::SRV as u8 > 0 }
        pub fn drive_fault(&self) -> bool { self.val & Bitflags::DF as u8 > 0 }
        pub fn ready(&self) -> bool { self.val & Bitflags::RDY as u8 > 0 }
        pub fn busy(&self) -> bool { self.val & Bitflags::BSY as u8 > 0 }
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
        pub fn address_mark_not_found(&self) -> bool { self.val & Bitflags::AMNF as u8 > 0 }
        pub fn track_zero_not_found(&self) -> bool { self.val & Bitflags::TKZNF as u8 > 0 }
        pub fn aborted_command(&self) -> bool { self.val & Bitflags::ABRT as u8 > 0 }
        pub fn media_change_request(&self) -> bool { self.val & Bitflags::MCR as u8 > 0 }
        pub fn id_not_found(&self) -> bool { self.val & Bitflags::IDNF as u8 > 0 }
        pub fn media_changed(&self) -> bool { self.val & Bitflags::MC as u8 > 0 }
        pub fn uncorrectable_data(&self) -> bool { self.val & Bitflags::UNC as u8 > 0 }
        pub fn bad_block(&self) -> bool { self.val & Bitflags::BBK as u8 > 0 }
    }
}

#[derive(Clone, Copy)]
pub enum Disk {
    Primary,
    Secondary,
    None,
}

pub static DISK_IO_BASES: [u16; 2] = [ATA_IO_PORT_PRIMARY, ATA_IO_PORT_SECONDARY];
pub static DISK_CONTROL_BASES: [u16; 2] = [ATA_CONTROL_PORT_PRIMARY, ATA_CONTROL_PORT_SECONDARY];

pub struct Driver {
    status: status::Status,
    disk: Disk,
}

impl Driver {
    pub fn new() -> Driver {
        let disk = Disk::Primary;
        let mut p = Port::new(DISK_IO_BASES[disk as u8 as usize] + IOPortRead::StatusRegister as u16);
        let status = status::Status { val: unsafe { p.read() } };
        Driver { status, disk }
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
    pub fn read(&mut self, buf: &mut [u16], lba: u32, sector_count: u8) {
        println!("In read");
        self.wait_bsy();
        let mut dh_reg = Port::new(DISK_IO_BASES[self.disk as u8 as usize] + IOPortRead::DriveAndHeadRegister as u16);
        let mut sec_count_reg = Port::new(DISK_IO_BASES[self.disk as u8 as usize] + IOPortRead::SectorCountRegister as u16);
        let mut lba_lo_reg = Port::new(DISK_IO_BASES[self.disk as u8 as usize] + IOPortRead::SectorNumberRegister as u16);
        let mut lba_mid_reg = Port::new(DISK_IO_BASES[self.disk as u8 as usize] + IOPortRead::CylinderLowRegister as u16);
        let mut lba_high_reg = Port::new(DISK_IO_BASES[self.disk as u8 as usize] + IOPortRead::CylinderHighRegister as u16);
        let mut cmd_reg = Port::new(DISK_IO_BASES[self.disk as u8 as usize] + IOPortWrite::CommandRegister as u16);
        let mut data_reg = Port::new(DISK_IO_BASES[self.disk as u8 as usize] + IOPortRead::DataRegister as u16);

        unsafe {
            dh_reg.write(0xE0 | ((lba >> 24) & 0xF)); // Hardcode  'master' (0xE0)
            sec_count_reg.write(sector_count);
            lba_lo_reg.write((lba & 0xFF) as u8);
            lba_mid_reg.write((lba >> 8 & 0xFF) as u8);
            lba_high_reg.write((lba >> 16 & 0xFF) as u8);
            cmd_reg.write(READ_COMMAND);
            
            println!("Set up, Starting read");

            for sec in 0..sector_count as usize {
                println!("Waiting to read sector");
                self.wait_bsy();
                println!("Waiting for ready");
                self.wait_drq();
                println!("Reading sector");
                for word in 0..256 {
                    buf[sec * 256 + word as usize] = data_reg.read();
                }
            }
        }   
        println!("Finished read");
    }
    pub fn write(&mut self, data: &mut [u16], lba: u32, sector_count: u8) {
        self.wait_bsy();
        let mut dh_reg = Port::new(DISK_IO_BASES[self.disk as u8 as usize] + IOPortRead::DriveAndHeadRegister as u16);
        let mut sec_count_reg = Port::new(DISK_IO_BASES[self.disk as u8 as usize] + IOPortRead::SectorCountRegister as u16);
        let mut lba_lo_reg = Port::new(DISK_IO_BASES[self.disk as u8 as usize] + IOPortRead::SectorNumberRegister as u16);
        let mut lba_mid_reg = Port::new(DISK_IO_BASES[self.disk as u8 as usize] + IOPortRead::CylinderLowRegister as u16);
        let mut lba_high_reg = Port::new(DISK_IO_BASES[self.disk as u8 as usize] + IOPortRead::CylinderHighRegister as u16);
        let mut cmd_reg = Port::new(DISK_IO_BASES[self.disk as u8 as usize] + IOPortWrite::CommandRegister as u16);
        let mut data_reg = Port::new(DISK_IO_BASES[self.disk as u8 as usize] + IOPortRead::DataRegister as u16);

        unsafe {
            dh_reg.write(0xE0 | ((lba >> 24) & 0xF)); // Hardcode  'master' (0xE0)
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
        let mut p = Port::new(DISK_IO_BASES[self.disk as u8 as usize] + IOPortRead::StatusRegister as u16);
        self.status = status::Status { val: unsafe { p.read() } };
    }
}

lazy_static! {
    pub static ref DRIVER: Mutex<Driver> = Mutex::new(Driver::new());
}

