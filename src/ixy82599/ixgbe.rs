use crate::memory;
use crate::packet;
use crate::link;

use std::collections::VecDeque;
use std::error::Error;
use std::mem;
use std::os::unix::io::RawFd;
use std::ptr;
use std::thread;
use std::time::{Duration, Instant};
use std::cmp;

use super::constants::*;
// use super::interrupts::*;
// use super::memory::*;
// use super::vfio::*;

use super::pci::pci_map_resource;
// use super::vfio::VFIO_PCI_BAR0_REGION_INDEX;
use super::DeviceStats;
// use super::Interrupts;
use super::IxyDevice;
use super::MAX_QUEUES;

const DRIVER_NAME: &str = "ixy-ixgbe";

const NUM_RX_QUEUE_ENTRIES: usize = 512;
const NUM_TX_QUEUE_ENTRIES: usize = 512;
const TX_CLEAN_BATCH: usize = 32;

fn wrap_ring(index: usize, ring_size: usize) -> usize {
    (index + 1) & (ring_size - 1)
}

pub struct IxgbeDevice {
    pci_addr: String,
    addr: *mut u8,
    len: usize,
    num_rx_queues: u16,
    num_tx_queues: u16,
    rx_queues: Vec<IxgbeRxQueue>,
    tx_queues: Vec<IxgbeTxQueue>,
}

struct IxgbeRxQueue {
    descriptors: *mut ixgbe_adv_rx_desc,
    num_descriptors: usize,
    bufs_in_use: Vec<*mut packet::Packet>,
    rx_index: usize,
}

struct IxgbeTxQueue {
    descriptors: *mut ixgbe_adv_tx_desc,
    num_descriptors: usize,
    bufs_in_use: VecDeque<*mut packet::Packet>,
    clean_index: usize,
    tx_index: usize,
}

impl IxyDevice for IxgbeDevice {
    /// Returns an initialized `IxgbeDevice` on success.
    ///
    /// # Panics
    /// Panics if `num_rx_queues` or `num_tx_queues` exceeds `MAX_QUEUES`.
    fn init(
        pci_addr: &str,
        num_rx_queues: u16,
        num_tx_queues: u16,
        _interrupt_timeout: i16,
    ) -> Result<IxgbeDevice, Box<dyn Error>> {
        if unsafe { libc::getuid() } != 0 {
            println!("not running as root, this will probably fail");
        }

        assert!(
            num_rx_queues <= MAX_QUEUES,
            "cannot configure {} rx queues: limit is {}",
            num_rx_queues,
            MAX_QUEUES
        );
        assert!(
            num_tx_queues <= MAX_QUEUES,
            "cannot configure {} tx queues: limit is {}",
            num_tx_queues,
            MAX_QUEUES
        );

        // map device registers
        let (addr, len) = pci_map_resource(pci_addr)?;

        // initialize RX and TX queue
        let rx_queues = Vec::with_capacity(num_rx_queues as usize);
        let tx_queues = Vec::with_capacity(num_tx_queues as usize);

        // create the IxyDevice
        let mut dev = IxgbeDevice {
            pci_addr: pci_addr.to_string(),
            addr,
            len,
            num_rx_queues,
            num_tx_queues,
            rx_queues,
            tx_queues
        };

        dev.reset_and_init(pci_addr)?;

        Ok(dev)
    }

    /// Returns the driver's name of this device.
    fn get_driver_name(&self) -> &str {
        DRIVER_NAME
    }

    /// Returns the card's iommu capability.
    fn is_card_iommu_capable(&self) -> bool {
        false
    }

    /// Returns VFIO container file descriptor or [`None`] if IOMMU is not available.
    fn get_vfio_container(&self) -> Option<RawFd> {
        None
    }

    /// Returns the pci address of this device.
    fn get_pci_addr(&self) -> &str {
        &self.pci_addr
    }

    /// Returns the mac address of this device.
    fn get_mac_addr(&self) -> [u8; 6] {
        let low = self.get_reg32(IXGBE_RAL(0));
        let high = self.get_reg32(IXGBE_RAH(0));

        [
            (low & 0xff) as u8,
            (low >> 8 & 0xff) as u8,
            (low >> 16 & 0xff) as u8,
            (low >> 24) as u8,
            (high & 0xff) as u8,
            (high >> 8 & 0xff) as u8,
        ]
    }

    /// Sets the mac address of this device.
    fn set_mac_addr(&self, mac: [u8; 6]) {
        let low: u32 = u32::from(mac[0])
            + (u32::from(mac[1]) << 8)
            + (u32::from(mac[2]) << 16)
            + (u32::from(mac[3]) << 24);
        let high: u32 = u32::from(mac[4]) + (u32::from(mac[5]) << 8);

        self.set_reg32(IXGBE_RAL(0), low);
        self.set_reg32(IXGBE_RAH(0), high);
    }

    /// Pushes up to `num_packets` received `Packet`s onto `buffer`.
    fn rx_batch(
        &mut self,
        queue_id: u32,
        output: &mut link::Link,
        num_packets: usize,
    ) -> usize {
        let mut rx_index;
        let mut last_rx_index;
        let mut received_packets = 0;

        {
            let queue = self
                .rx_queues
                .get_mut(queue_id as usize)
                .expect("invalid rx queue id");

            rx_index = queue.rx_index;
            last_rx_index = queue.rx_index;

            for i in 0..num_packets {
                let desc = unsafe { queue.descriptors.add(rx_index) as *mut ixgbe_adv_rx_desc };
                let status =
                    unsafe { ptr::read_volatile(&mut (*desc).wb.upper.status_error as *mut u32) };

                if (status & IXGBE_RXDADV_STAT_DD) == 0 {
                    break;
                }

                if (status & IXGBE_RXDADV_STAT_EOP) == 0 {
                    panic!("increase buffer size or decrease MTU")
                }

                // get next packet
                let mut p = unsafe { Box::from_raw(queue.bufs_in_use[rx_index]) };
                p.length = unsafe { ptr::read_volatile(&(*desc).wb.upper.length as *const u16) };

                // replace currently used buffer with new buffer (packet)
                let mut np = packet::allocate();
                queue.bufs_in_use[rx_index] = &mut *np; mem::forget(np);

                link::transmit(output, p);

                unsafe {
                    ptr::write_volatile(
                        &mut (*desc).read.pkt_addr as *mut u64,
                        memory::virtual_to_physical((*queue.bufs_in_use[rx_index]).data.as_ptr())
                    );
                    ptr::write_volatile(&mut (*desc).read.hdr_addr as *mut u64, 0);
                }

                last_rx_index = rx_index;
                rx_index = wrap_ring(rx_index, queue.num_descriptors);
                received_packets = i + 1;
            }
        }

        if rx_index != last_rx_index {
            self.set_reg32(IXGBE_RDT(queue_id), last_rx_index as u32);
            self.rx_queues[queue_id as usize].rx_index = rx_index;
        }

        received_packets
    }

    /// Pops as many packets as possible from `packets` to put them into the device`s tx queue.
    fn tx_batch(&mut self, queue_id: u32, input: &mut link::Link) -> usize {
        let mut sent = 0;

        {
            let mut queue = self
                .tx_queues
                .get_mut(queue_id as usize)
                .expect("invalid tx queue id");

            let mut cur_index = queue.tx_index;
            let clean_index = clean_tx_queue(&mut queue);

            while !link::empty(input) {
                let next_index = wrap_ring(cur_index, queue.num_descriptors);

                if clean_index == next_index {
                    // tx queue of device is full
                    break;
                }

                let mut p = link::receive(input);

                queue.tx_index = wrap_ring(queue.tx_index, queue.num_descriptors);

                unsafe {
                    ptr::write_volatile(
                        &mut (*queue.descriptors.add(cur_index)).read.buffer_addr as *mut u64,
                        memory::virtual_to_physical(p.data.as_ptr())
                    );
                    ptr::write_volatile(
                        &mut (*queue.descriptors.add(cur_index)).read.cmd_type_len as *mut u32,
                        IXGBE_ADVTXD_DCMD_EOP
                            | IXGBE_ADVTXD_DCMD_RS
                            | IXGBE_ADVTXD_DCMD_IFCS
                            | IXGBE_ADVTXD_DCMD_DEXT
                            | IXGBE_ADVTXD_DTYP_DATA
                            | p.length as u32,
                    );
                    ptr::write_volatile(
                        &mut (*queue.descriptors.add(cur_index)).read.olinfo_status as *mut u32,
                        (p.length as u32) << IXGBE_ADVTXD_PAYLEN_SHIFT,
                    );
                }

                queue.bufs_in_use.push_back(&mut *p);
                mem::forget(p);

                cur_index = next_index;
                sent += 1;
            }
        }

        self.set_reg32(
            IXGBE_TDT(queue_id),
            self.tx_queues[queue_id as usize].tx_index as u32,
        );

        sent
    }

    /// Reads the stats of this device into `stats`.
    fn read_stats(&self, stats: &mut DeviceStats) {
        let rx_pkts = u64::from(self.get_reg32(IXGBE_GPRC));
        let tx_pkts = u64::from(self.get_reg32(IXGBE_GPTC));
        let rx_bytes =
            u64::from(self.get_reg32(IXGBE_GORCL)) + (u64::from(self.get_reg32(IXGBE_GORCH)) << 32);
        let tx_bytes =
            u64::from(self.get_reg32(IXGBE_GOTCL)) + (u64::from(self.get_reg32(IXGBE_GOTCH)) << 32);

        stats.rx_pkts += rx_pkts;
        stats.tx_pkts += tx_pkts;
        stats.rx_bytes += rx_bytes;
        stats.tx_bytes += tx_bytes;
    }

    /// Resets the stats of this device.
    fn reset_stats(&mut self) {
        self.get_reg32(IXGBE_GPRC);
        self.get_reg32(IXGBE_GPTC);
        self.get_reg32(IXGBE_GORCL);
        self.get_reg32(IXGBE_GORCH);
        self.get_reg32(IXGBE_GOTCL);
        self.get_reg32(IXGBE_GOTCH);
    }

    /// Returns the link speed of this device.
    fn get_link_speed(&self) -> u16 {
        let speed = self.get_reg32(IXGBE_LINKS);
        if (speed & IXGBE_LINKS_UP) == 0 {
            return 0;
        }
        match speed & IXGBE_LINKS_SPEED_82599 {
            IXGBE_LINKS_SPEED_100_82599 => 100,
            IXGBE_LINKS_SPEED_1G_82599 => 1000,
            IXGBE_LINKS_SPEED_10G_82599 => 10000,
            _ => 0,
        }
    }
}

impl IxgbeDevice {
    /// Resets and initializes this device.
    fn reset_and_init(&mut self, _pci_addr: &str) -> Result<(), Box<dyn Error>> {
        // info!("resetting device {}", pci_addr);
        // section 4.6.3.1 - disable all interrupts
        self.disable_interrupts();

        // section 4.6.3.2
        self.set_reg32(IXGBE_CTRL, IXGBE_CTRL_RST_MASK);
        self.wait_clear_reg32(IXGBE_CTRL, IXGBE_CTRL_RST_MASK);
        thread::sleep(Duration::from_millis(10));

        // section 4.6.3.1 - disable interrupts again after reset
        self.disable_interrupts();

        let _mac = self.get_mac_addr();
        // info!("initializing device {}", pci_addr);
        // info!(
        //     "mac address: {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
        //     mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]
        // );

        // section 4.6.3 - wait for EEPROM auto read completion
        self.wait_set_reg32(IXGBE_EEC, IXGBE_EEC_ARD);

        // section 4.6.3 - wait for dma initialization done
        self.wait_set_reg32(IXGBE_RDRXCTL, IXGBE_RDRXCTL_DMAIDONE);

        // skip last step from 4.6.3 - we don't want interrupts

        // section 4.6.4 - initialize link (auto negotiation)
        self.init_link();

        // section 4.6.5 - statistical counters
        // reset-on-read registers, just read them once
        self.reset_stats();

        // section 4.6.7 - init rx
        self.init_rx()?;

        // section 4.6.8 - init tx
        self.init_tx()?;

        for i in 0..self.num_rx_queues {
            self.start_rx_queue(i)?;
        }

        for i in 0..self.num_tx_queues {
            self.start_tx_queue(i)?;
        }

        // enable promisc mode by default to make testing easier
        self.set_promisc(true);

        // wait some time for the link to come up
        self.wait_for_link();

        Ok(())
    }

    // sections 4.6.7
    /// Initializes the rx queues of this device.
    fn init_rx(&mut self) -> Result<(), Box<dyn Error>> {
        // disable rx while re-configuring it
        self.clear_flags32(IXGBE_RXCTRL, IXGBE_RXCTRL_RXEN);

        // section 4.6.11.3.4 - allocate all queues and traffic to PB0
        self.set_reg32(IXGBE_RXPBSIZE(0), IXGBE_RXPBSIZE_128KB);
        for i in 1..8 {
            self.set_reg32(IXGBE_RXPBSIZE(i), 0);
        }

        // enable CRC offloading
        self.set_flags32(IXGBE_HLREG0, IXGBE_HLREG0_RXCRCSTRP);
        self.set_flags32(IXGBE_RDRXCTL, IXGBE_RDRXCTL_CRCSTRIP);

        // accept broadcast packets
        self.set_flags32(IXGBE_FCTRL, IXGBE_FCTRL_BAM);

        // configure queues, same for all queues
        for i in 0..self.num_rx_queues {
            // debug!("initializing rx queue {}", i);
            // enable advanced rx descriptors
            self.set_reg32(
                IXGBE_SRRCTL(u32::from(i)),
                (self.get_reg32(IXGBE_SRRCTL(u32::from(i))) & !IXGBE_SRRCTL_DESCTYPE_MASK)
                    | IXGBE_SRRCTL_DESCTYPE_ADV_ONEBUF,
            );
            // let nic drop packets if no rx descriptor is available instead of buffering them
            self.set_flags32(IXGBE_SRRCTL(u32::from(i)), IXGBE_SRRCTL_DROP_EN);

            // section 7.1.9 - setup descriptor ring
            let ring_size_bytes =
                (NUM_RX_QUEUE_ENTRIES) as usize * mem::size_of::<ixgbe_adv_rx_desc>();

            let dma_virt = memory::dma_alloc(ring_size_bytes, 128);
            let dma_phys = memory::virtual_to_physical(dma_virt);

            // initialize to 0xff to prevent rogue memory accesses on premature dma activation
            unsafe {
                memset(dma_virt as *mut u8, ring_size_bytes, 0xff);
            }

            self.set_reg32(
                IXGBE_RDBAL(u32::from(i)),
                (dma_phys as u64 & 0xffff_ffff) as u32,
            );
            self.set_reg32(IXGBE_RDBAH(u32::from(i)), (dma_phys as u64 >> 32) as u32);
            self.set_reg32(IXGBE_RDLEN(u32::from(i)), ring_size_bytes as u32);

            // debug!("rx ring {} phys addr: {:#x}", i, dma.phys);
            // debug!("rx ring {} virt addr: {:p}", i, dma.virt);

            // set ring to empty at start
            self.set_reg32(IXGBE_RDH(u32::from(i)), 0);
            self.set_reg32(IXGBE_RDT(u32::from(i)), 0);

            let rx_queue = IxgbeRxQueue {
                descriptors: dma_virt as *mut ixgbe_adv_rx_desc,
                num_descriptors: NUM_RX_QUEUE_ENTRIES,
                rx_index: 0,
                bufs_in_use: Vec::with_capacity(NUM_RX_QUEUE_ENTRIES),
            };

            self.rx_queues.push(rx_queue);
        }

        // last sentence of section 4.6.7 - set some magic bits
        self.set_flags32(IXGBE_CTRL_EXT, IXGBE_CTRL_EXT_NS_DIS);

        // probably a broken feature, this flag is initialized with 1 but has to be set to 0
        for i in 0..self.num_rx_queues {
            self.clear_flags32(IXGBE_DCA_RXCTRL(u32::from(i)), 1 << 12);
        }

        // start rx
        self.set_flags32(IXGBE_RXCTRL, IXGBE_RXCTRL_RXEN);

        Ok(())
    }

    // section 4.6.8
    /// Initializes the tx queues of this device.
    fn init_tx(&mut self) -> Result<(), Box<dyn Error>> {
        // crc offload and small packet padding
        self.set_flags32(IXGBE_HLREG0, IXGBE_HLREG0_TXCRCEN | IXGBE_HLREG0_TXPADEN);

        // section 4.6.11.3.4 - set default buffer size allocations
        self.set_reg32(IXGBE_TXPBSIZE(0), IXGBE_TXPBSIZE_40KB);
        for i in 1..8 {
            self.set_reg32(IXGBE_TXPBSIZE(i), 0);
        }

        // required when not using DCB/VTd
        self.set_reg32(IXGBE_DTXMXSZRQ, 0xffff);
        self.clear_flags32(IXGBE_RTTDCS, IXGBE_RTTDCS_ARBDIS);

        // configure queues
        for i in 0..self.num_tx_queues {
            // debug!("initializing tx queue {}", i);
            // section 7.1.9 - setup descriptor ring
            let ring_size_bytes =
                NUM_TX_QUEUE_ENTRIES as usize * mem::size_of::<ixgbe_adv_tx_desc>();

            let dma_virt = memory::dma_alloc(ring_size_bytes, 128);
            let dma_phys = memory::virtual_to_physical(dma_virt);
            unsafe {
                memset(dma_virt as *mut u8, ring_size_bytes, 0xff);
            }

            self.set_reg32(
                IXGBE_TDBAL(u32::from(i)),
                (dma_phys as u64 & 0xffff_ffff) as u32,
            );
            self.set_reg32(IXGBE_TDBAH(u32::from(i)), (dma_phys as u64 >> 32) as u32);
            self.set_reg32(IXGBE_TDLEN(u32::from(i)), ring_size_bytes as u32);

            // debug!("tx ring {} phys addr: {:#x}", i, dma.phys);
            // debug!("tx ring {} virt addr: {:p}", i, dma.virt);

            // descriptor writeback magic values, important to get good performance and low PCIe overhead
            // see 7.2.3.4.1 and 7.2.3.5 for an explanation of these values and how to find good ones
            // we just use the defaults from DPDK here, but this is a potentially interesting point for optimizations
            let mut txdctl = self.get_reg32(IXGBE_TXDCTL(u32::from(i)));
            // there are no defines for this in constants.rs for some reason
            // pthresh: 6:0, hthresh: 14:8, wthresh: 22:16
            txdctl &= !(0x3F | (0x3F << 8) | (0x3F << 16));
            txdctl |= 36 | (8 << 8) | (4 << 16);

            self.set_reg32(IXGBE_TXDCTL(u32::from(i)), txdctl);

            let tx_queue = IxgbeTxQueue {
                descriptors: dma_virt as *mut ixgbe_adv_tx_desc,
                bufs_in_use: VecDeque::with_capacity(NUM_TX_QUEUE_ENTRIES),
                num_descriptors: NUM_TX_QUEUE_ENTRIES,
                clean_index: 0,
                tx_index: 0,
            };

            self.tx_queues.push(tx_queue);
        }

        // final step: enable DMA
        self.set_reg32(IXGBE_DMATXCTL, IXGBE_DMATXCTL_TE);

        Ok(())
    }

    /// Sets the rx queues` descriptors and enables the queues.
    fn start_rx_queue(&mut self, queue_id: u16) -> Result<(), Box<dyn Error>> {
        // debug!("starting rx queue {}", queue_id);

        {
            let queue = &mut self.rx_queues[queue_id as usize];

            if queue.num_descriptors & (queue.num_descriptors - 1) != 0 {
                return Err("number of queue entries must be a power of 2".into());
            }

            for i in 0..queue.num_descriptors {
                let mut np = packet::allocate();

                unsafe {
                    ptr::write_volatile(
                        &mut (*queue.descriptors.add(i)).read.pkt_addr as *mut u64,
                        memory::virtual_to_physical(np.data.as_ptr())
                    );

                    ptr::write_volatile(
                        &mut (*queue.descriptors.add(i)).read.hdr_addr as *mut u64,
                        0,
                    );
                }

                // we need to remember which descriptor entry belongs to which mempool entry
                queue.bufs_in_use.push(&mut *np); mem::forget(np);
            }
        }

        let queue = &self.rx_queues[queue_id as usize];

        // enable queue and wait if necessary
        self.set_flags32(IXGBE_RXDCTL(u32::from(queue_id)), IXGBE_RXDCTL_ENABLE);
        self.wait_set_reg32(IXGBE_RXDCTL(u32::from(queue_id)), IXGBE_RXDCTL_ENABLE);

        // rx queue starts out full
        self.set_reg32(IXGBE_RDH(u32::from(queue_id)), 0);

        // was set to 0 before in the init function
        self.set_reg32(
            IXGBE_RDT(u32::from(queue_id)),
            (queue.num_descriptors - 1) as u32,
        );

        Ok(())
    }

    /// Enables the tx queues.
    fn start_tx_queue(&mut self, queue_id: u16) -> Result<(), Box<dyn Error>> {
        // debug!("starting tx queue {}", queue_id);

        {
            let queue = &mut self.tx_queues[queue_id as usize];

            if queue.num_descriptors & (queue.num_descriptors - 1) != 0 {
                return Err("number of queue entries must be a power of 2".into());
            }
        }

        // tx queue starts out empty
        self.set_reg32(IXGBE_TDH(u32::from(queue_id)), 0);
        self.set_reg32(IXGBE_TDT(u32::from(queue_id)), 0);

        // enable queue and wait if necessary
        self.set_flags32(IXGBE_TXDCTL(u32::from(queue_id)), IXGBE_TXDCTL_ENABLE);
        self.wait_set_reg32(IXGBE_TXDCTL(u32::from(queue_id)), IXGBE_TXDCTL_ENABLE);

        Ok(())
    }

    // see section 4.6.4
    /// Initializes the link of this device.
    fn init_link(&self) {
        // link auto-configuration register should already be set correctly, we're resetting it anyway
        self.set_reg32(
            IXGBE_AUTOC,
            (self.get_reg32(IXGBE_AUTOC) & !IXGBE_AUTOC_LMS_MASK) | IXGBE_AUTOC_LMS_10G_SERIAL,
        );
        self.set_reg32(
            IXGBE_AUTOC,
            (self.get_reg32(IXGBE_AUTOC) & !IXGBE_AUTOC_10G_PMA_PMD_MASK) | IXGBE_AUTOC_10G_XAUI,
        );
        // negotiate link
        self.set_flags32(IXGBE_AUTOC, IXGBE_AUTOC_AN_RESTART);
        // datasheet wants us to wait for the link here, but we can continue and wait afterwards
    }

    /// Waits for the link to come up.
    fn wait_for_link(&self) {
        // info!("waiting for link");
        let time = Instant::now();
        let mut speed = self.get_link_speed();
        while speed == 0 && time.elapsed().as_secs() < 10 {
            thread::sleep(Duration::from_millis(100));
            speed = self.get_link_speed();
        }
        // info!("link speed is {} Mbit/s", self.get_link_speed());
    }

    /// Enables or disables promisc mode of this device.
    fn set_promisc(&self, enabled: bool) {
        if enabled {
            // info!("enabling promisc mode");
            self.set_flags32(IXGBE_FCTRL, IXGBE_FCTRL_MPE | IXGBE_FCTRL_UPE);
        } else {
            // info!("disabling promisc mode");
            self.clear_flags32(IXGBE_FCTRL, IXGBE_FCTRL_MPE | IXGBE_FCTRL_UPE);
        }
    }

    /// Returns the register at `self.addr` + `reg`.
    ///
    /// # Panics
    ///
    /// Panics if `self.addr` + `reg` does not belong to the mapped memory of the pci device.
    fn get_reg32(&self, reg: u32) -> u32 {
        assert!(
            reg as usize <= self.len - 4 as usize,
            "memory access out of bounds"
        );

        unsafe { ptr::read_volatile((self.addr as usize + reg as usize) as *mut u32) }
    }

    /// Sets the register at `self.addr` + `reg` to `value`.
    ///
    /// # Panics
    ///
    /// Panics if `self.addr` + `reg` does not belong to the mapped memory of the pci device.
    fn set_reg32(&self, reg: u32, value: u32) {
        assert!(
            reg as usize <= self.len - 4 as usize,
            "memory access out of bounds"
        );

        unsafe {
            ptr::write_volatile((self.addr as usize + reg as usize) as *mut u32, value);
        }
    }

    /// Sets the `flags` at `self.addr` + `reg`.
    fn set_flags32(&self, reg: u32, flags: u32) {
        self.set_reg32(reg, self.get_reg32(reg) | flags);
    }

    /// Clears the `flags` at `self.addr` + `reg`.
    fn clear_flags32(&self, reg: u32, flags: u32) {
        self.set_reg32(reg, self.get_reg32(reg) & !flags);
    }

    /// Waits for `self.addr` + `reg` to clear `value`.
    fn wait_clear_reg32(&self, reg: u32, value: u32) {
        loop {
            let current = self.get_reg32(reg);
            if (current & value) == 0 {
                break;
            }
            thread::sleep(Duration::from_millis(100));
        }
    }

    /// Waits for `self.addr` + `reg` to set `value`.
    fn wait_set_reg32(&self, reg: u32, value: u32) {
        loop {
            let current = self.get_reg32(reg);
            if (current & value) == value {
                break;
            }
            thread::sleep(Duration::from_millis(100));
        }
    }

    /// Clear all interrupt masks for all queues.
    fn clear_interrupts(&self) {
        // Clear interrupt mask
        self.set_reg32(IXGBE_EIMC, IXGBE_IRQ_CLEAR_MASK);
        self.get_reg32(IXGBE_EICR);
    }

    /// Disable all interrupts for all queues.
    fn disable_interrupts(&self) {
        // Clear interrupt mask to stop from interrupts being generated
        self.set_reg32(IXGBE_EIMS, 0x0000_0000);
        self.clear_interrupts();
    }
}

/// Removes multiples of `TX_CLEAN_BATCH` packets from `queue`.
fn clean_tx_queue(queue: &mut IxgbeTxQueue) -> usize {
    let mut clean_index = queue.clean_index;
    let cur_index = queue.tx_index;

    loop {
        let mut cleanable = cur_index as i32 - clean_index as i32;

        if cleanable < 0 {
            cleanable += queue.num_descriptors as i32;
        }

        if cleanable < TX_CLEAN_BATCH as i32 {
            break;
        }

        let mut cleanup_to = clean_index + TX_CLEAN_BATCH - 1;

        if cleanup_to >= queue.num_descriptors {
            cleanup_to -= queue.num_descriptors;
        }

        let status = unsafe {
            ptr::read_volatile(&(*queue.descriptors.add(cleanup_to)).wb.status as *const u32)
        };

        if (status & IXGBE_ADVTXD_STAT_DD) != 0 {
            for _ in 0..cmp::min(TX_CLEAN_BATCH, queue.bufs_in_use.len()) {
                packet::free(unsafe {
                    Box::from_raw(queue.bufs_in_use.pop_front().unwrap())
                });
            }

            clean_index = wrap_ring(cleanup_to, queue.num_descriptors);
        } else {
            break;
        }
    }

    queue.clean_index = clean_index;

    clean_index
}

// SNIPPED FROM ORIGINAL MEMORY.RS

/// Initializes `len` fields of type `T` at `addr` with `value`.
unsafe fn memset<T: Copy>(addr: *mut T, len: usize, value: T) {
    for i in 0..len {
        ptr::write_volatile(addr.add(i) as *mut T, value);
    }
}
