TOPDIR := $(HOME)/rpmbuild
SOURCES_DIR := $(TOPDIR)/SOURCES
SPEC := rpm-install.spec
NAME := rpm-install
VERSION := $(shell rpmspec -q --qf '%{VERSION}\n' --srpm $(SPEC) | head -n1)

all: srpm

srpm:
	mkdir -p "$(SOURCES_DIR)"
	rm -rf vendor .cargo
	cargo vendor vendor >/dev/null
	tar --zstd -cf "$(SOURCES_DIR)/$(NAME)-$(VERSION)-vendor.tar.zst" vendor
	rm -rf vendor .cargo
	spectool -g -R $(SPEC)
	rpmbuild -bs $(SPEC)

clean:
	rm -rf vendor .cargo
