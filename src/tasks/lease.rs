//! Vigila la concesión DHCP y corrige las subredes que dejan fuera al router.
//!
//! Con una máscara que no incluye al router (ver `crate::subnet`), se pasa a
//! una configuración estática con la misma dirección, el mismo router y los
//! mismos DNS, pero con el prefijo ampliado hasta incluirlo. Al perder el
//! enlace se vuelve a DHCP para no conservar una concesión vieja. En una red
//! normal no cambia nada: solo registra la concesión.

use embassy_executor::task;
use embassy_net::{ConfigV4, DhcpConfig, Ipv4Cidr, Stack, StaticConfigV4};

use crate::subnet::prefix_including_gateway;

#[task]
pub async fn lease_task(stack: Stack<'static>) {
    loop {
        stack.wait_link_up().await;
        stack.wait_config_up().await;

        let mut widened = false;
        if let Some(config) = stack.config_v4() {
            info!(
                "[NET] DHCP lease: address {}, gateway {:?}, DNS {:?}",
                config.address,
                config.gateway,
                config.dns_servers.as_slice()
            );
            if let Some(gateway) = config.gateway {
                let address = config.address.address();
                if let Some(prefix) = prefix_including_gateway(
                    address.octets(),
                    config.address.prefix_len(),
                    gateway.octets(),
                ) {
                    info!(
                        "[NET] Gateway {} is outside {}; using /{} so ARP replies are accepted",
                        gateway, config.address, prefix
                    );
                    stack.set_config_v4(ConfigV4::Static(StaticConfigV4 {
                        address: Ipv4Cidr::new(address, prefix),
                        gateway: Some(gateway),
                        dns_servers: config.dns_servers.clone(),
                    }));
                    widened = true;
                }
            }
        }

        stack.wait_link_down().await;
        if widened {
            stack.set_config_v4(ConfigV4::Dhcp(DhcpConfig::default()));
        }
    }
}
