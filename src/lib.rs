#![no_std]
//! Driver to write characters to LCD displays with a LM1602 connected via i2c like [this one] with
//! 16x2 characters. It requires a I2C instance implementing [`embedded_hal::blocking::i2c::Write`]
//! and a instance to delay execution with [`embedded_hal::blocking::delay::DelayMs`].
//!
//! Usage:
//! ```
//! const LCD_ADDRESS: u8 = 0x27; // Address depends on hardware, see link below
//!
//! // Create a I2C instance, needs to implement embedded_hal::blocking::i2c::Write, this
//! // particular uses the arduino_hal crate for avr microcontrollers like the arduinos.
//! let dp = arduino_hal::Peripherals::take().unwrap();
//! let pins = arduino_hal::pins!(dp);
//! let mut i2c = arduino_hal::I2c::new(
//!     dp.TWI, //
//!     pins.a4.into_pull_up_input(), // use respective pins
//!     pins.a5.into_pull_up_input(),
//!     50000,
//! );
//! let mut delay = arduino_hal::Delay::new();
//!
//! let mut lcd = lcd_lcm1602_i2c::Lcd::new(&mut i2c, &mut delay)
//!     .address(LCD_ADDRESS)
//!     .cursor_on(false) // no visible cursor
//!     .rows(2) // two rows
//!     .init().unwrap();
//! ```
//!
//! This [site][lcd address] describes how to find the address of your LCD devices.
//!
//! [this one]: https://funduinoshop.com/elektronische-module/displays/lcd/16x02-i2c-lcd-modul-hintergrundbeleuchtung-blau
//! [lcd address]: https://www.ardumotive.com/i2clcden.html

use core::marker::PhantomData;

use embedded_hal::blocking::{delay::DelayMs, i2c};

use ufmt_write::uWrite;

/// API to write to the LCD.
/// PhantomData<D> used to ensure correct Delay without having to take ownership of delay.
pub struct Lcd<'a, I, D>
where
    I: i2c::Write,
    D: DelayMs<u8>,
{
    i2c: &'a mut I,
    address: u8,
    rows: u8,
    backlight_state: Backlight,
    cursor_on: bool,
    cursor_blink: bool,
    phantomdata: PhantomData<D>,
}

pub enum DisplayControl {
    Off = 0x00,
    CursorBlink = 0x01,
    CursorOn = 0x02,
    DisplayOn = 0x04,
}

#[derive(Copy, Clone)]
pub enum Backlight {
    Off = 0x00,
    On = 0x08,
}

#[repr(u8)]
#[derive(Copy, Clone)]
enum Mode {
    Cmd = 0x00,
    Data = 0x01,
    DisplayControl = 0x08,
    FunctionSet = 0x20,
}

enum Commands {
    Clear = 0x01,
    ReturnHome = 0x02,
    ShiftCursor = 0x14,
}

enum BitMode {
    Bit4 = 0x00,
    Bit8 = 0x10,
}

impl<'a, I, D> Lcd<'a, I, D>
where
    I: i2c::Write,
    D: DelayMs<u8>,
{
    /// Create new instance with only the I2C and delay instance.
    pub fn new(i2c: &'a mut I, backlight_state: Backlight) -> Self {
        Self {
            i2c,
            backlight_state,
            address: 0,
            rows: 0,
            cursor_blink: false,
            cursor_on: false,
            phantomdata: PhantomData,
        }
    }

    /// Set I2C address, see [lcd address].
    ///
    /// [lcd address]: https://badboi.dev/rust,/microcontrollers/2020/11/09/i2c-hello-world.html
    pub fn address(mut self, address: u8) -> Self {
        self.address = address;
        self
    }

    pub fn cursor_on(mut self, on: bool) -> Self {
        self.cursor_on = on;
        self
    }

    pub fn cursor_blink(mut self, on: bool) -> Self {
        self.cursor_blink = on;
        self
    }

    /// Zero based number of rows.
    pub fn rows(mut self, rows: u8) -> Self {
        self.rows = rows;
        self
    }

    /// Initializes the hardware.
    ///
    /// Actual procedure is a bit obscure. This one was compiled from this [blog post],
    /// corresponding [code] and the [datasheet].
    ///
    /// [datasheet]: https://www.openhacks.com/uploadsproductos/eone-1602a1.pdf
    /// [code]: https://github.com/jalhadi/i2c-hello-world/blob/main/src/main.rs
    /// [blog post]: https://badboi.dev/rust,/microcontrollers/2020/11/09/i2c-hello-world.html
    pub fn init(mut self, delay: &mut D) -> Result<Self, <I as i2c::Write>::Error> {
        // Initial delay to wait for init after power on.
        delay.delay_ms(80);

        // Init with 8 bit mode
        let mode_8bit = Mode::FunctionSet as u8 | BitMode::Bit8 as u8;
        self.write4bits(mode_8bit)?;
        delay.delay_ms(5);
        self.write4bits(mode_8bit)?;
        delay.delay_ms(5);
        self.write4bits(mode_8bit)?;
        delay.delay_ms(5);

        // Switch to 4 bit mode
        let mode_4bit = Mode::FunctionSet as u8 | BitMode::Bit4 as u8;
        self.write4bits(mode_4bit)?;

        // Display setup
        // Set mode either 2 or four lines
        // TODO: Verify for 20x4 screen
        let lines = if self.rows == 0 { 0x00 } else { 0x08 };
        self.send(Mode::FunctionSet as u8 | lines, Mode::Cmd)?;

        // Set display on, optionally turn on cursor and cursor blink
        let mut display_ctrl = DisplayControl::DisplayOn as u8;
        if self.cursor_on {
            display_ctrl |= DisplayControl::CursorOn as u8;

            if self.cursor_blink {
                display_ctrl |= DisplayControl::CursorBlink as u8;
            }
        }
        self.send(Mode::DisplayControl as u8 | display_ctrl, Mode::Cmd)?;

        // Clear Display, also moves cursor to top left
        self.send(Mode::Cmd as u8 | Commands::Clear as u8, Mode::Cmd)?;

        // Entry right: shifting cursor moves to right
        self.send(0x04, Mode::Cmd)?;
        self.backlight(self.backlight_state)?;
        Ok(self)
    }

    fn write4bits(&mut self, data: u8) -> Result<(), <I as i2c::Write>::Error> {
        self.i2c.write(
            self.address,
            &[data | DisplayControl::DisplayOn as u8 | self.backlight_state as u8],
        )?;
        self.i2c.write(
            self.address,
            &[DisplayControl::Off as u8 | self.backlight_state as u8],
        )?;
        Ok(())
    }

    fn send(&mut self, data: u8, mode: Mode) -> Result<(), <I as i2c::Write>::Error> {
        let high_bits: u8 = data & 0xf0;
        let low_bits: u8 = (data << 4) & 0xf0;
        self.write4bits(high_bits | mode as u8)?;
        self.write4bits(low_bits | mode as u8)?;
        Ok(())
    }

    pub fn backlight(&mut self, backlight: Backlight) -> Result<(), <I as i2c::Write>::Error> {
        self.backlight_state = backlight;
        self.i2c.write(
            self.address,
            &[DisplayControl::DisplayOn as u8 | backlight as u8],
        )
    }

    /// Write string to display.
    pub fn write_str(&mut self, data: &str) -> Result<(), <I as i2c::Write>::Error> {
        for c in data.chars() {
            self.send(c as u8, Mode::Data)?;
        }
        Ok(())
    }

    /// Clear the display
    pub fn clear(&mut self) -> Result<(), <I as i2c::Write>::Error> {
        self.send(Commands::Clear as u8, Mode::Cmd)?;
        Ok(())
    }

    /// Return cursor to upper left corner, i.e. (0,0).
    pub fn return_home(&mut self, delay: &mut D) -> Result<(), <I as i2c::Write>::Error> {
        self.send(Commands::ReturnHome as u8, Mode::Cmd)?;
        delay.delay_ms(10);
        Ok(())
    }

    /// Set the cursor to (rows, col). Coordinates are zero-based.
    pub fn set_cursor(&mut self, row: u8, col: u8, delay: &mut D) -> Result<(), <I as i2c::Write>::Error> {
        self.return_home(delay)?;
        let shift: u8 = row * 40 + col;
        for _i in 0..shift {
            self.send(Commands::ShiftCursor as u8, Mode::Cmd)?;
        }
        Ok(())
    }
}