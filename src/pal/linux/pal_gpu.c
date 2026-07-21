#include "../pal.h"
#include "../../runtime/runtime.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <dirent.h>
#include <unistd.h>

static char *read_sysfs(const char *path) {
    FILE *f = fopen(path, "r");
    if (!f) return rt_managed_from_slice("", 0);
    char buf[1024];
    size_t n = fread(buf, 1, sizeof(buf) - 1, f);
    fclose(f);
    while (n > 0 && (buf[n-1] == '\n' || buf[n-1] == '\r')) n--;
    return rt_managed_from_slice(buf, n);
}

static const char *pci_vendor_name(const char *id) {
    if (strcmp(id, "0x10de") == 0) return "NVIDIA";
    if (strcmp(id, "0x1002") == 0) return "AMD";
    if (strcmp(id, "0x8086") == 0) return "Intel";
    if (strcmp(id, "0x1414") == 0) return "Microsoft";
    return id;
}

static int64_t read_vram_total(const char *card_path) {
    char path[512];
    snprintf(path, sizeof(path), "%s/device/mem_info_vram_total", card_path);
    FILE *f = fopen(path, "r");
    if (f) {
        long long val = 0;
        if (fscanf(f, "%lld", &val) == 1) { fclose(f); return val; }
        fclose(f);
    }
    return -1;
}

static char *detect_driver(const char *card_path) {
    char path[512];
    snprintf(path, sizeof(path), "%s/device/driver", card_path);
    char target[512];
    ssize_t len = readlink(path, target, sizeof(target) - 1);
    if (len > 0) {
        target[len] = '\0';
        const char *base = strrchr(target, '/');
        if (base) base++; else base = target;
        return rt_managed_from_slice(base, strlen(base));
    }
    return rt_managed_from_slice("unknown", 7);
}

static void dict_set_str(void *dict, const char *key, const char *value) {
    rt_dicts_set(dict, key, (void *)value);
}

static void dict_set_i64(void *dict, const char *key, int64_t value) {
    rt_dicts_set_i64(dict, key, value);
}

/* Returns a dict[str str] with GPU info. */
void *pal_gpu_snapshot(void) {
    void *result = rt_dict_ensure_kind(NULL, MIRE_KIND_STR, MIRE_KIND_STR);

    DIR *d = opendir("/sys/class/drm");
    if (!d) {
        dict_set_str(result, "available", "false");
        dict_set_str(result, "count", "0");
        return result;
    }

    struct dirent *ent;
    char cards[8][256];
    int count = 0;

    while ((ent = readdir(d)) != NULL && count < 8) {
        if (strncmp(ent->d_name, "card", 4) != 0) continue;
        if (strchr(ent->d_name + 4, '-') != NULL) continue;

        char card_path[512];
        snprintf(card_path, sizeof(card_path), "/sys/class/drm/%s", ent->d_name);

        char test_path[512];
        snprintf(test_path, sizeof(test_path), "%s/device/vendor", card_path);
        if (access(test_path, R_OK) != 0) continue;

        strncpy(cards[count], card_path, 255);
        cards[count][255] = '\0';
        count++;
    }
    closedir(d);

    if (count == 0) {
        dict_set_str(result, "available", "false");
        dict_set_str(result, "count", "0");
        return result;
    }

    dict_set_str(result, "available", "true");
    char count_buf[16];
    snprintf(count_buf, sizeof(count_buf), "%d", count);
    dict_set_str(result, "count", count_buf);

    for (int i = 0; i < count; i++) {
        char vpath[512], dpath[512];
        snprintf(vpath, sizeof(vpath), "%s/device/vendor", cards[i]);
        snprintf(dpath, sizeof(dpath), "%s/device/device", cards[i]);

        char *vendor = read_sysfs(vpath);
        char *device = read_sysfs(dpath);
        char *driver = detect_driver(cards[i]);
        int64_t vram = read_vram_total(cards[i]);

        char key[64];

        snprintf(key, sizeof(key), "gpu%d_vendor", i);
        dict_set_str(result, key, vendor);

        snprintf(key, sizeof(key), "gpu%d_device", i);
        dict_set_str(result, key, device);

        snprintf(key, sizeof(key), "gpu%d_driver", i);
        dict_set_str(result, key, driver);

        snprintf(key, sizeof(key), "gpu%d_name", i);
        char name_buf[256];
        snprintf(name_buf, sizeof(name_buf), "%s %s", pci_vendor_name(vendor), device);
        dict_set_str(result, key, rt_managed_from_slice(name_buf, strlen(name_buf)));

        if (vram >= 0) {
            snprintf(key, sizeof(key), "gpu%d_vram", i);
            dict_set_i64(result, key, vram);
        }
    }

    return result;
}
