FROM archlinux
RUN echo 'Server = https://mirrors.cloud.tencent.com/archlinux/$repo/os/$arch' | tee /etc/pacman.d/mirrorlist && \
    printf '\n[archlinuxcn]\nServer = https://repo.archlinuxcn.org/$arch\n' | tee -a /etc/pacman.conf
RUN pacman-key --init && pacman-key --populate && yes | pacman -Syyu --noconfirm archlinuxcn-keyring && pacman -Sc
RUN pacman -S --noconfirm gcc cmake ninja qemu-headless qemu-headless-arch-extra wget && pacman -Sc --noconfirm
RUN wget http://musl.cc/mipsel-linux-musl-cross.tgz && \
    tar xvzf mipsel-linux-musl-cross.tgz && \
    rm -f mipsel-linux-musl-cross.tgz && \
    ln -s /mipsel-linux-musl-cross/bin/mipsel-linux-musl-cc /usr/bin/mcc   && \
    ln -s /mipsel-linux-musl-cross/bin/mipsel-linux-musl-gcc /usr/bin/mgcc && \
    ln -s /mipsel-linux-musl-cross/bin/mipsel-linux-musl-g++ /usr/bin/mg++ && \
    ln -s /mipsel-linux-musl-cross/bin/mipsel-linux-musl-c++ /usr/bin/mc++ && \
    ln -s /mipsel-linux-musl-cross/bin/mipsel-linux-musl-nm /usr/bin/mnm   && \
    ln -s /mipsel-linux-musl-cross/bin/mipsel-linux-musl-objdump /usr/bin/mobjdump
CMD tail --pid 1 -f /dev/null