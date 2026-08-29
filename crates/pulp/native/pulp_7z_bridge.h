#ifndef PULP_7Z_BRIDGE_H
#define PULP_7Z_BRIDGE_H

/*
 * This header is the private, fixed-width C ABI boundary of pulp.
 * It intentionally contains no filesystem path and no 7-Zip C++ type.
 */

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct Pulp7zBridge Pulp7zBridge;

#define PULP7Z_OK 0
#define PULP7Z_INVALID_ARGUMENT (-1)
#define PULP7Z_CALLBACK_ERROR (-2)
#define PULP7Z_PASSWORD_DECLINED (-3)
#define PULP7Z_NATIVE_ERROR (-4)
#define PULP7Z_STREAM_UNAVAILABLE (-5)

typedef int32_t (*Pulp7zCreateObjectFn)(const void* class_id,
                                        const void* interface_id,
                                        void**      object);
typedef int32_t (*Pulp7zGetNumberOfFormatsFn)(uint32_t* number);
typedef int32_t (*Pulp7zGetHandlerPropertyFn)(uint32_t property_id, void* value);
typedef int32_t (*Pulp7zGetHandlerProperty2Fn)(uint32_t index, uint32_t property_id, void* value);
typedef int32_t (*Pulp7zGetNumberOfMethodsFn)(uint32_t* number);
typedef int32_t (*Pulp7zGetMethodPropertyFn)(uint32_t index, uint32_t property_id, void* value);

typedef struct Pulp7zError {
    int32_t status;
    char    message[512];
} Pulp7zError;

typedef struct Pulp7zFormatInfo {
    uint32_t       index;
    uint8_t        class_id[16];
    uint8_t        has_class_id;
    uint32_t       flags;
    uint32_t       time_flags;
    uint32_t       signature_offset;
    uint8_t        update;
    uint8_t        keep_name;
    uint8_t        alt_streams;
    uint8_t        nt_secure;
    const char*    name;
    uint32_t       name_len;
    const char*    extension;
    uint32_t       extension_len;
    const char*    add_extension;
    uint32_t       add_extension_len;
    const uint8_t* signature;
    uint32_t       signature_len;
    const uint8_t* multi_signature;
    uint32_t       multi_signature_len;
} Pulp7zFormatInfo;

typedef int32_t (*Pulp7zFormatCallback)(void* user, const Pulp7zFormatInfo* info);

typedef struct Pulp7zMethodInfo {
    uint32_t    index;
    uint8_t     id[16];
    uint8_t     has_id;
    uint8_t     decoder[16];
    uint8_t     has_decoder;
    uint8_t     encoder[16];
    uint8_t     has_encoder;
    const char* name;
    uint32_t    name_len;
    const char* description;
    uint32_t    description_len;
    uint8_t     decoder_assigned;
    uint8_t     encoder_assigned;
    uint8_t     is_filter;
} Pulp7zMethodInfo;

typedef int32_t (*Pulp7zMethodCallback)(void* user, const Pulp7zMethodInfo* info);

typedef struct Pulp7zEntryInfo {
    uint32_t    index;
    const char* path;
    uint32_t    path_len;
    uint8_t     is_dir;
    uint8_t     encrypted;
    uint8_t     link_kind;
    uint8_t     has_size;
    uint8_t     has_pack_size;
    uint8_t     has_mtime;
    uint8_t     has_attrib;
    uint8_t     has_posix_attrib;
    uint8_t     has_crc;
    uint64_t    size;
    uint64_t    pack_size;
    int64_t     mtime_unix_ns;
    uint32_t    attrib;
    uint32_t    posix_attrib;
    uint32_t    crc;
    const char* method;
    uint32_t    method_len;
    const char* link_target;
    uint32_t    link_target_len;
} Pulp7zEntryInfo;

typedef int32_t (*Pulp7zEntryCallback)(void* user, const Pulp7zEntryInfo* info);

typedef int32_t (*Pulp7zReadCallback)(void*     user,
                                      uint8_t*  data,
                                      uint32_t  size,
                                      uint32_t* processed);
typedef int32_t (*Pulp7zSeekCallback)(void*     user,
                                      int64_t   offset,
                                      uint32_t  origin,
                                      uint64_t* position);
typedef int32_t (*Pulp7zWriteCallback)(void*          user,
                                       const uint8_t* data,
                                       uint32_t       size,
                                       uint32_t*      processed);

typedef struct Pulp7zInputCallbacks {
    void*              user;
    Pulp7zReadCallback read;
    Pulp7zSeekCallback seek;
} Pulp7zInputCallbacks;

typedef struct Pulp7zOutputCallbacks {
    void*               user;
    Pulp7zWriteCallback write;
    Pulp7zSeekCallback  seek;
} Pulp7zOutputCallbacks;

typedef int32_t (*Pulp7zProgressCallback)(void*    user,
                                          uint64_t total,
                                          uint64_t completed,
                                          uint32_t phase);
typedef int32_t (*Pulp7zPasswordCallback)(void*     user,
                                          uint32_t  reason,
                                          uint32_t  attempt,
                                          uint8_t*  password,
                                          uint32_t  capacity,
                                          uint32_t* length);

typedef void (*Pulp7zVolumeCloseCallback)(void* user);

typedef struct Pulp7zVolumeCallbacks {
    void*                     user;
    Pulp7zReadCallback         read;
    Pulp7zSeekCallback         seek;
    Pulp7zVolumeCloseCallback  close;
} Pulp7zVolumeCallbacks;

typedef int32_t (*Pulp7zOpenVolumeCallback)(void*                  user,
                                             const char*            name,
                                             uint32_t               name_len,
                                             Pulp7zVolumeCallbacks* callbacks);

typedef struct Pulp7zOpenCallbacks {
    void*                  user;
    Pulp7zProgressCallback progress;
    Pulp7zPasswordCallback password;
    Pulp7zOpenVolumeCallback volume;
    const char*             archive_name;
    uint32_t                archive_name_len;
} Pulp7zOpenCallbacks;

typedef int32_t (*Pulp7zExtractBeginCallback)(void*                  user,
                                              const Pulp7zEntryInfo* info,
                                              uint32_t               ask_mode,
                                              uint32_t*              decision);
typedef int32_t (*Pulp7zExtractWriteCallback)(void*          user,
                                              const uint8_t* data,
                                              uint32_t       size,
                                              uint32_t*      processed);
typedef int32_t (*Pulp7zExtractFinishCallback)(void*                  user,
                                               const Pulp7zEntryInfo* info,
                                               int32_t                operation_result,
                                               uint64_t               bytes);

typedef struct Pulp7zExtractCallbacks {
    void*                       user;
    Pulp7zProgressCallback      progress;
    Pulp7zPasswordCallback      password;
    Pulp7zOpenVolumeCallback    volume;
    const char*                 archive_name;
    uint32_t                    archive_name_len;
    Pulp7zExtractBeginCallback  begin;
    Pulp7zExtractWriteCallback  write;
    Pulp7zExtractFinishCallback finish;
} Pulp7zExtractCallbacks;

typedef int32_t (*Pulp7zSourceEntryCallback)(void* user, uint32_t index, Pulp7zEntryInfo* info);
typedef int32_t (*Pulp7zSourceReadCallback)(
    void* user, uint32_t index, uint8_t* data, uint32_t size, uint32_t* processed);

typedef struct Pulp7zSourceCallbacks {
    void*                     user;
    uint32_t                  count;
    Pulp7zSourceEntryCallback entry;
    Pulp7zSourceReadCallback  read;
    Pulp7zProgressCallback    progress;
    Pulp7zPasswordCallback    password;
} Pulp7zSourceCallbacks;

typedef struct Pulp7zUpdateOptions {
    const char* method;
    uint32_t    method_len;
    int32_t     level;
    int32_t     solid;
    int32_t     header_encryption;
} Pulp7zUpdateOptions;

int32_t pulp7z_bridge_create(Pulp7zCreateObjectFn        create_object,
                             Pulp7zGetNumberOfFormatsFn  get_number_of_formats,
                             Pulp7zGetHandlerPropertyFn  get_handler_property,
                             Pulp7zGetHandlerProperty2Fn get_handler_property2,
                             Pulp7zGetNumberOfMethodsFn  get_number_of_methods,
                             Pulp7zGetMethodPropertyFn   get_method_property,
                             Pulp7zBridge**              out_bridge,
                             Pulp7zError*                out_error);

void pulp7z_bridge_destroy(Pulp7zBridge* bridge);

int32_t pulp7z_bridge_enumerate_formats(Pulp7zBridge*        bridge,
                                        Pulp7zFormatCallback callback,
                                        void*                user,
                                        Pulp7zError*         out_error);

int32_t pulp7z_bridge_enumerate_methods(Pulp7zBridge*        bridge,
                                        Pulp7zMethodCallback callback,
                                        void*                user,
                                        Pulp7zError*         out_error);

int32_t pulp7z_bridge_list(Pulp7zBridge*               bridge,
                           const uint8_t               class_id[16],
                           const Pulp7zInputCallbacks* input,
                           const Pulp7zOpenCallbacks*  open_callbacks,
                           Pulp7zEntryCallback         callback,
                           void*                       user,
                           Pulp7zError*                out_error);

int32_t pulp7z_bridge_probe(Pulp7zBridge*               bridge,
                            const uint8_t               class_id[16],
                            const Pulp7zInputCallbacks* input,
                            const Pulp7zOpenCallbacks*  open_callbacks,
                            Pulp7zError*                out_error);

int32_t pulp7z_bridge_copy_entry(Pulp7zBridge*                bridge,
                                 const uint8_t                class_id[16],
                                 const Pulp7zInputCallbacks*  input,
                                 const Pulp7zOpenCallbacks*   open_callbacks,
                                 uint32_t                     index,
                                 const Pulp7zOutputCallbacks* output,
                                 Pulp7zError*                  out_error);

int32_t pulp7z_bridge_extract(Pulp7zBridge*                 bridge,
                              const uint8_t                 class_id[16],
                              const Pulp7zInputCallbacks*   input,
                              const uint32_t*               indices,
                              uint32_t                      index_count,
                              int32_t                       test_mode,
                              const Pulp7zExtractCallbacks* callbacks,
                              Pulp7zError*                  out_error);

int32_t pulp7z_bridge_update(Pulp7zBridge*                bridge,
                             const uint8_t                class_id[16],
                             const Pulp7zOutputCallbacks* output,
                             const Pulp7zSourceCallbacks* source,
                             const Pulp7zUpdateOptions*   options,
                             Pulp7zError*                 out_error);

#ifdef __cplusplus
}
#endif

#endif
