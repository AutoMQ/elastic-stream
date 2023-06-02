# Issue
https://repost.aws/questions/QUuota-4iYQ029aEF_Oq14zg/kernel-null-pointer-exception-while-using-io-uring-with-the-latest-ubuntu-ami

# Ubuntu: Install a Previous Kernel
```bash
sudo apt install linux-image-5.19.0-1024-aws
```

# Ubuntu: Install Kernel and Set GRUB Default Kernel
## Install Kernel
Install the default kernel:
```bash
sudo apt install linux-generic
```

## Set GRUB Default Kernel
1. Find entrance from `/boot/grub/grub.cfg`
    - Get the $menuentry_id_option:
        ```bash
        grep submenu /boot/grub/grub.cfg
        ```
        Example output:
        ```bash
        submenu 'Advanced options for Ubuntu' $menuentry_id_option 'gnulinux-advanced-4591a659-55e2-4bec-8dbe-d98bd9e489cf' {
        ```
        `'gnulinux-advanced-4591a659-55e2-4bec-8dbe-d98bd9e489cf'` is what we need.
    - Get the specific kernel option:
        ```bash
        grep gnulinux-4.15.0 /boot/grub/grub.cfg
        ```
        Example output:
        ```bash
        menuentry 'Ubuntu, with Linux 4.15.0-126-generic' --class ubuntu --class gnu-linux --class gnu --class os $menuentry_id_option 'gnulinux-4.15.0-126-generic-advanced-4591a659-55e2-4bec-8dbe-d98bd9e489cf' {
        ```
        `'gnulinux-4.15.0-126-generic-advanced-4591a659-55e2-4bec-8dbe-d98bd9e489cf'` is what we need.
2. Set GRUB_DEFAULT in `/etc/default/grub`
    - Join two entry strings obtained above by '>', set to `GRUB_DEFAULT`.
        ```bash
        GRUB_DEFAULT='gnulinux-advanced-4591a659-55e2-4bec-8dbe-d98bd9e489cf>gnulinux-4.15.0-126-generic-advanced-4591a659-55e2-4bec-8dbe-d98bd9e489cf'
        ```
3. Update grub
   ```bash
   sudo update-grub
   ```
4. Reboot the machine