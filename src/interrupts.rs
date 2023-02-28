use crate::{gdt, hlt_loop, print, println, time::TIMER};
use lazy_static::lazy_static;
use pic8259::ChainedPics;
use spin;
use x86_64::structures::idt::{InterruptDescriptorTable, InterruptStackFrame, PageFaultErrorCode};

pub const PIC_1_OFFSET: u8 = 32;
pub const PIC_2_OFFSET: u8 = PIC_1_OFFSET + 8;

#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum InterruptIndex {
    Timer = PIC_1_OFFSET,
    Keyboard,
    PIC2,
    Serial1,
    Serial2,
    ParallelPort2,
    Floppy,
    ParallelPort1,
    RTC,
    ACPI,
    Unused1,
    Unused2,
    Mouse,
    CoProcessor,
    PrimaryAta,
    SecondaryAta,
}

impl InterruptIndex {
    fn as_u8(self) -> u8 {
        self as u8
    }

    fn as_usize(self) -> usize {
        usize::from(self.as_u8())
    }
}

pub static PICS: spin::Mutex<ChainedPics> =
    spin::Mutex::new(unsafe { ChainedPics::new(PIC_1_OFFSET, PIC_2_OFFSET) });

lazy_static! {
    static ref IDT: InterruptDescriptorTable = {
        let mut idt = InterruptDescriptorTable::new();
        idt.breakpoint.set_handler_fn(breakpoint_handler);
        idt.page_fault.set_handler_fn(page_fault_handler);
        //unsafe {
        //    idt.general_protection_fault
        //    .set_handler_fn(general_protection_handler);
        //}
        unsafe {
            idt.double_fault
                .set_handler_fn(double_fault_handler)
                .set_stack_index(gdt::DOUBLE_FAULT_IST_INDEX);
        }
        idt[InterruptIndex::Timer.as_usize()].set_handler_fn(timer_interrupt_handler);
        idt[InterruptIndex::Keyboard.as_usize()].set_handler_fn(keyboard_interrupt_handler);
        idt[InterruptIndex::PrimaryAta.as_usize()].set_handler_fn(primary_ata_interrupt_handler);
        idt[InterruptIndex::SecondaryAta.as_usize()].set_handler_fn(secondary_ata_interrupt_handler);
        idt[InterruptIndex::PIC2.as_usize()].set_handler_fn(pic2_interrupt_handler);
        idt[InterruptIndex::Serial1.as_usize()].set_handler_fn(serial1_interrupt_handler);
        idt[InterruptIndex::Serial2.as_usize()].set_handler_fn(serial2_interrupt_handler);
        idt[InterruptIndex::ParallelPort2.as_usize()].set_handler_fn(parallel_port2_interrupt_handler);
        idt[InterruptIndex::Floppy.as_usize()].set_handler_fn(floppy_interrupt_handler);
        idt[InterruptIndex::ParallelPort1.as_usize()].set_handler_fn(parallel_port1_interrupt_handler);
        idt[InterruptIndex::RTC.as_usize()].set_handler_fn(rtc_interrupt_handler);
        idt[InterruptIndex::ACPI.as_usize()].set_handler_fn(acpi_interrupt_handler);
        idt[InterruptIndex::Unused1.as_usize()].set_handler_fn(unused1_interrupt_handler);
        idt[InterruptIndex::Unused2.as_usize()].set_handler_fn(unused2_interrupt_handler);
        idt[InterruptIndex::Mouse.as_usize()].set_handler_fn(mouse_interrupt_handler);
        idt[InterruptIndex::CoProcessor.as_usize()].set_handler_fn(coprocessor_interrupt_handler);
        idt
    };
}

pub fn init_idt() {
    IDT.load();
}

extern "x86-interrupt" fn breakpoint_handler(stack_frame: InterruptStackFrame) {
    println!("EXCEPTION: BREAKPOINT\n{:#?}", stack_frame);
}

extern "x86-interrupt" fn page_fault_handler(
    stack_frame: InterruptStackFrame,
    error_code: PageFaultErrorCode,
) {
    use x86_64::registers::control::Cr2;

    println!("EXCEPTION: PAGE FAULT");
    println!("Accessed Address: {:?}", Cr2::read());
    println!("Error Code: {:?}", error_code);
    println!("{:#?}", stack_frame);
    hlt_loop();
}

extern "x86-interrupt" fn double_fault_handler(
    stack_frame: InterruptStackFrame,
    _error_code: u64,
) -> ! {
    panic!("EXCEPTION: DOUBLE FAULT\n{:#?}", stack_frame);
}

extern "x86-interrupt" fn general_protection_handler(stack_frame: InterruptStackFrame,
    error_code: u64,
) {
    println!("EXCEPTION: GENERAL PROTECTION FAULT\nERROR CODE: {}\n{:#?}", error_code, stack_frame);
    //panic!();
}

extern "x86-interrupt" fn timer_interrupt_handler(_stack_frame: InterruptStackFrame) {
    x86_64::instructions::interrupts::without_interrupts(||{
        *TIMER.lock() += 1;
    });
    unsafe {
        PICS.lock()
            .notify_end_of_interrupt(InterruptIndex::Timer.as_u8());
    }
}

extern "x86-interrupt" fn keyboard_interrupt_handler(_stack_frame: InterruptStackFrame) {
    use x86_64::instructions::port::Port;

    let mut port = Port::new(0x60);
    let scancode: u8 = unsafe { port.read() };
    crate::task::keyboard::add_scancode(scancode);

    unsafe {
        PICS.lock()
            .notify_end_of_interrupt(InterruptIndex::Keyboard.as_u8());
    }
}

extern "x86-interrupt" fn primary_ata_interrupt_handler(_stack_frame: InterruptStackFrame) {
    x86_64::instructions::interrupts::without_interrupts(|| { println!("Primary ATA Interrupt") } );
    unsafe {
        PICS.lock()
            .notify_end_of_interrupt(InterruptIndex::PrimaryAta.as_u8());
    }
}

extern "x86-interrupt" fn secondary_ata_interrupt_handler(_stack_frame: InterruptStackFrame) {
    x86_64::instructions::interrupts::without_interrupts(|| { println!("Secondary ATA Interrupt") } );
    unsafe {
        PICS.lock()
            .notify_end_of_interrupt(InterruptIndex::SecondaryAta.as_u8());
    }
}

extern "x86-interrupt" fn pic2_interrupt_handler(_stack_frame: InterruptStackFrame) {
    x86_64::instructions::interrupts::without_interrupts(|| { println!("Secondary PIC Interrupt") } );
    unsafe {
        PICS.lock()
            .notify_end_of_interrupt(InterruptIndex::PIC2.as_u8());
    }
}

extern "x86-interrupt" fn serial1_interrupt_handler(_stack_frame: InterruptStackFrame) {
    x86_64::instructions::interrupts::without_interrupts(|| { println!("Serial 1 Interrupt") } );
    unsafe {
        PICS.lock()
            .notify_end_of_interrupt(InterruptIndex::Serial1.as_u8());
    }
}

extern "x86-interrupt" fn serial2_interrupt_handler(_stack_frame: InterruptStackFrame) {
    x86_64::instructions::interrupts::without_interrupts(|| { println!("Serial 2 Interrupt") } );
    unsafe {
        PICS.lock()
            .notify_end_of_interrupt(InterruptIndex::Serial2.as_u8());
    }
}

extern "x86-interrupt" fn parallel_port2_interrupt_handler(_stack_frame: InterruptStackFrame) {
    x86_64::instructions::interrupts::without_interrupts(|| { println!("Parallel Port 2 Interrupt") } );
    unsafe {
        PICS.lock()
            .notify_end_of_interrupt(InterruptIndex::ParallelPort2.as_u8());
    }
}

extern "x86-interrupt" fn floppy_interrupt_handler(_stack_frame: InterruptStackFrame) {
    x86_64::instructions::interrupts::without_interrupts(|| { println!("Floppy Interrupt") } );
    unsafe {
        PICS.lock()
            .notify_end_of_interrupt(InterruptIndex::Floppy.as_u8());
    }
}

extern "x86-interrupt" fn parallel_port1_interrupt_handler(_stack_frame: InterruptStackFrame) {
    x86_64::instructions::interrupts::without_interrupts(|| { println!("Parallel Port 1 Interrupt") } );
    unsafe {
        PICS.lock()
            .notify_end_of_interrupt(InterruptIndex::ParallelPort1.as_u8());
    }
}

extern "x86-interrupt" fn rtc_interrupt_handler(_stack_frame: InterruptStackFrame) {
    x86_64::instructions::interrupts::without_interrupts(|| { println!("RTC Interrupt") } );
    unsafe {
        PICS.lock()
            .notify_end_of_interrupt(InterruptIndex::RTC.as_u8());
    }
}

extern "x86-interrupt" fn acpi_interrupt_handler(_stack_frame: InterruptStackFrame) {
    x86_64::instructions::interrupts::without_interrupts(|| { println!("ACPI Interrupt") } );
    unsafe {
        PICS.lock()
            .notify_end_of_interrupt(InterruptIndex::ACPI.as_u8());
    }
}

extern "x86-interrupt" fn unused2_interrupt_handler(_stack_frame: InterruptStackFrame) {
    x86_64::instructions::interrupts::without_interrupts(|| { println!("Unused 2 Interrupt") } );
    unsafe {
        PICS.lock()
            .notify_end_of_interrupt(InterruptIndex::Unused2.as_u8());
    }
}

extern "x86-interrupt" fn unused1_interrupt_handler(_stack_frame: InterruptStackFrame) {
    x86_64::instructions::interrupts::without_interrupts(|| { println!("Unused 1 Interrupt") } );
    unsafe {
        PICS.lock()
            .notify_end_of_interrupt(InterruptIndex::Unused1.as_u8());
    }
}

extern "x86-interrupt" fn mouse_interrupt_handler(_stack_frame: InterruptStackFrame) {
    x86_64::instructions::interrupts::without_interrupts(|| { println!("Mouse Interrupt") } );
    unsafe {
        PICS.lock()
            .notify_end_of_interrupt(InterruptIndex::Mouse.as_u8());
    }
}

extern "x86-interrupt" fn coprocessor_interrupt_handler(_stack_frame: InterruptStackFrame) {
    x86_64::instructions::interrupts::without_interrupts(|| { println!("Coprocessor Interrupt") } );
    unsafe {
        PICS.lock()
            .notify_end_of_interrupt(InterruptIndex::CoProcessor.as_u8());
    }
}

#[test_case]
fn test_breakpoint_exception() {
    // invoke a breakpoint exception
    x86_64::instructions::interrupts::int3();
}
