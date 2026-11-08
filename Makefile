# Root delegator Makefile
.PHONY: all help build release run test package install open stats clean

all release build run test package install open clean:
	$(MAKE) -C RustUtilityApp $@

stats:
	@./RustUtilityApp/scripts/download-stats.sh

help:
	@$(MAKE) -C RustUtilityApp help
