use rusb::{Context, Device, DeviceHandle, Result, UsbContext};
use std::time::Duration;
use systemstat::{Platform, System};

const VID: u16 = 0x264a;
const PID: u16 = 0x226f;

fn main() -> Result<()> {
    let mut context = Context::new()?;
    let (mut device, mut handle) =
        open_device(&mut context, VID, PID).expect("Did not find USB device");

    // print_device_info(&mut handle)?;

    let endpoints = find_readable_endpoints(&mut device)?;
    let endpoint = endpoints
        .first()
        .expect("No Configurable endpoint found on device");
    let has_kernel_driver = match handle.kernel_driver_active(0) {
        Ok(true) => {
            handle.detach_kernel_driver(0)?;
            true
        }
        _ => false,
    };
    println!("has kernel driver? {}", has_kernel_driver);

    let endpoint_in = &endpoints[0];
    let endpoint_out = &endpoints[1];

    configure_endpoint(&mut handle, &endpoint)?;

    set_idle(&mut handle).ok();
    // set_fan_speed(&mut handle, 2, 100, endpoint_out.address);
    main_loop(&mut handle, endpoint_out.address, endpoint_in.address);

    // let fan_speed = get_fan_speed(
    //     &mut handle,
    //     0x02,
    //     endpoint_out.address,
    //     endpoint_in.address,
    // );
    // println!("{:?}", fan_speed?);

    handle.release_interface(0)?;
    if has_kernel_driver {
        handle.attach_kernel_driver(0)?;
    }
    Ok(())
}

fn main_loop<T: UsbContext>(handle: &mut DeviceHandle<T>, endpoint_out: u8, endpoint_in: u8) {
    let mut t_cpu: u8 = 0;
    // let mut previous_t_cpu: u8 = 0;
    let fans: [u8; 2] = [2, 3];
    let mut fans_speed: u8 = 0;
    let sys = System::new();

    loop {
        match sys.cpu_temp() {
            Ok(cpu_temp) => {
                // previous_t_cpu = t_cpu;
                t_cpu = cpu_temp as u8;
            }
            Err(_x) => (),
        }
        fans_speed = ((100.0 * t_cpu as f32) / 90.0) as u8;
        if fans_speed > 100 {
            fans_speed = 100;
        }
        // println!("{}", );
        for fan in fans {
            set_fan_speed(handle, fan, fans_speed as u8, endpoint_out);

            // let fan_speed = get_fan_speed(handle, fan, endpoint_out, endpoint_in);
            let fan_speed = match get_fan_speed(handle, fan, endpoint_out, endpoint_in) {
                Ok(rpm) => rpm,
                _ => 0,
            };
            if fan_speed > 0 {
                println!("Fan{}: {:?}rpm/{}%", fan, fan_speed, fans_speed);
            }
            
        }
    }
}

fn open_device<T: UsbContext>(
    context: &mut T,
    vid: u16,
    pid: u16,
) -> Option<(Device<T>, DeviceHandle<T>)> {
    let devices = match context.devices() {
        Ok(d) => d,
        Err(_) => return None,
    };

    for device in devices.iter() {
        let device_desc = match device.device_descriptor() {
            Ok(d) => d,
            Err(_) => continue,
        };

        if device_desc.vendor_id() == vid && device_desc.product_id() == pid {
            match device.open() {
                Ok(handle) => return Some((device, handle)),
                Err(_) => continue,
            }
        }
    }

    None
}

#[derive(Debug)]
struct Endpoint {
    config: u8,
    iface: u8,
    setting: u8,
    address: u8,
}
// returns all readable endpoints for given usb device and descriptor
fn find_readable_endpoints<T: UsbContext>(device: &mut Device<T>) -> Result<Vec<Endpoint>> {
    let device_desc = device.device_descriptor()?;
    let mut endpoints = vec![];
    for n in 0..device_desc.num_configurations() {
        let config_desc = match device.config_descriptor(n) {
            Ok(c) => c,
            Err(_) => continue,
        };
        // println!("{:#?}", config_desc);
        for interface in config_desc.interfaces() {
            for interface_desc in interface.descriptors() {
                // println!("{:#?}", interface_desc);
                for endpoint_desc in interface_desc.endpoint_descriptors() {
                    // println!("{:#?}", endpoint_desc);
                    endpoints.push(Endpoint {
                        config: config_desc.number(),
                        iface: interface_desc.interface_number(),
                        setting: interface_desc.setting_number(),
                        address: endpoint_desc.address(),
                    });
                }
            }
        }
    }

    Ok(endpoints)
}

fn configure_endpoint<T: UsbContext>(
    handle: &mut DeviceHandle<T>,
    endpoint: &Endpoint,
) -> Result<()> {
    match handle.set_active_configuration(endpoint.config) {
        Ok(_) => (),
        Err(err) => println!("set_active_configuration Err: {}", err),
    }
    match handle.claim_interface(endpoint.iface) {
        Ok(_) => (),
        Err(err) => println!("claim_interface Err: {}", err),
    }
    handle.set_alternate_setting(endpoint.iface, endpoint.setting)
}

fn set_idle<T: UsbContext>(handle: &mut DeviceHandle<T>) -> Result<usize> {
    let timeout = Duration::from_secs(1);
    // Const values are picked directly from the package capture data
    const REQUEST_TYPE: u8 = 0x21;
    const REQUEST: u8 = 0x0A;
    const VALUE: u16 = 0x0000;
    const INDEX: u16 = 0x0000;
    handle.write_control(REQUEST_TYPE, REQUEST, VALUE, INDEX, &[], timeout)
}

fn get_fan_speed<T: UsbContext>(
    handle: &mut DeviceHandle<T>,
    fan_port: u8,
    endpoint_out: u8,
    endpoint_in: u8,
) -> Result<u16> {
    let timeout = Duration::from_secs(1);

    let mut data = [0u8; 64];
    data[0] = 0x33;
    data[1] = 0x51;
    data[2] = fan_port;
    handle.write_interrupt(endpoint_out, &data, timeout);

    let mut buf = [0u8; 64];
    handle
        .read_interrupt(endpoint_in, &mut buf, timeout)
        .map(|_| (((buf.to_vec())[6] as u16) << 8) + (buf.to_vec())[5] as u16)
    // let out = handle
    //     .read_interrupt(endpoint_in, &mut buf, timeout)
    //     .map(|_| buf.to_vec());
    // return out;
}

fn set_fan_speed<T: UsbContext>(
    handle: &mut DeviceHandle<T>,
    fan_port: u8,
    speed: u8,
    endpoint_out: u8,
) {
    let timeout = Duration::from_secs(1);

    let mut data = [0u8; 192];
    data[0] = 0x32;
    data[1] = 0x51;
    data[2] = fan_port;
    data[3] = 0x01;
    data[4] = speed;
    match handle.write_interrupt(endpoint_out, &data, timeout) {
        Ok(_) => (),
        Err(x) => print!("set_fan_speed Error: {x}\n"),
    }
}
