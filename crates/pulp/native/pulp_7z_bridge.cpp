#include "pulp_7z_bridge.h"

#include "sdk/CPP/7zip/Archive/IArchive.h"
#include "sdk/CPP/7zip/ICoder.h"
#include "sdk/CPP/7zip/IPassword.h"
#include "sdk/CPP/Common/MyCom.h"
#include "sdk/CPP/Common/MyWindows.h"

#include <algorithm>
#include <cstdint>
#include <cwchar>
#include <cstring>
#include <limits>
#include <string>
#include <vector>

struct Pulp7zBridge {
    Pulp7zCreateObjectFn        create_object;
    Pulp7zGetNumberOfFormatsFn  get_number_of_formats;
    Pulp7zGetHandlerPropertyFn  get_handler_property;
    Pulp7zGetHandlerProperty2Fn get_handler_property2;
    Pulp7zGetNumberOfMethodsFn  get_number_of_methods;
    Pulp7zGetMethodPropertyFn   get_method_property;
};

namespace {

constexpr UInt32 kAskExtract    = NArchive::NExtract::NAskMode::kExtract;
constexpr UInt32 kDecisionSkip  = 0;
constexpr UInt32 kDecisionWrite = 2;

void clear_error(Pulp7zError* error) {
    if (error != nullptr) {
        error->status     = PULP7Z_OK;
        error->message[0] = '\0';
    }
}

int32_t set_error(Pulp7zError* error, int32_t status, const char* message) {
    if (error != nullptr) {
        error->status = status;
        std::strncpy(
            error->message, message == nullptr ? "error" : message, sizeof(error->message) - 1U);
        error->message[sizeof(error->message) - 1U] = '\0';
    }
    return status;
}

int32_t set_hresult_error(Pulp7zError* error, HRESULT status, const char* message) {
    return set_error(error, static_cast<int32_t>(status), message);
}

bool failed(HRESULT status) {
    return status < 0;
}

void append_utf8(std::string& result, UInt32 code_point) {
    if (code_point <= 0x7fU) {
        result.push_back(static_cast<char>(code_point));
    } else if (code_point <= 0x7ffU) {
        result.push_back(static_cast<char>(0xc0U | (code_point >> 6U)));
        result.push_back(static_cast<char>(0x80U | (code_point & 0x3fU)));
    } else if (code_point <= 0xffffU) {
        result.push_back(static_cast<char>(0xe0U | (code_point >> 12U)));
        result.push_back(static_cast<char>(0x80U | ((code_point >> 6U) & 0x3fU)));
        result.push_back(static_cast<char>(0x80U | (code_point & 0x3fU)));
    } else if (code_point <= 0x10ffffU) {
        result.push_back(static_cast<char>(0xf0U | (code_point >> 18U)));
        result.push_back(static_cast<char>(0x80U | ((code_point >> 12U) & 0x3fU)));
        result.push_back(static_cast<char>(0x80U | ((code_point >> 6U) & 0x3fU)));
        result.push_back(static_cast<char>(0x80U | (code_point & 0x3fU)));
    } else {
        append_utf8(result, 0xfffdU);
    }
}

std::string bstr_to_utf8(BSTR value) {
    std::string result;
    if (value == nullptr) {
        return result;
    }
    const UINT length = SysStringLen(value);
    result.reserve(length);
    if constexpr (sizeof(wchar_t) == 2) {
        for (UINT index = 0; index < length; ++index) {
            UInt32 code_point = static_cast<UInt16>(value[index]);
            if (code_point >= 0xd800U && code_point <= 0xdbffU && index + 1 < length) {
                const UInt32 low = static_cast<UInt16>(value[index + 1]);
                if (low >= 0xdc00U && low <= 0xdfffU) {
                    code_point = 0x10000U + ((code_point - 0xd800U) << 10U) + (low - 0xdc00U);
                    ++index;
                }
            }
            append_utf8(result, code_point);
        }
    } else {
        for (UINT index = 0; index < length; ++index) {
            append_utf8(result, static_cast<UInt32>(value[index]));
        }
    }
    return result;
}

UInt32 decode_utf8_code_point(const char* data, size_t length, size_t& used) {
    if (length == 0) {
        used = 0;
        return 0xfffdU;
    }
    const auto first = static_cast<unsigned char>(data[0]);
    if (first < 0x80U) {
        used = 1;
        return first;
    }
    const auto continuation = [&](size_t index) -> UInt32 {
        return index < length && (static_cast<unsigned char>(data[index]) & 0xc0U) == 0x80U
                   ? static_cast<unsigned char>(data[index]) & 0x3fU
                   : 0xffffffffU;
    };
    if (first >= 0xc2U && first <= 0xdfU && length >= 2) {
        const UInt32 second = continuation(1);
        if (second != 0xffffffffU) {
            used = 2;
            return ((first & 0x1fU) << 6U) | second;
        }
    } else if (first >= 0xe0U && first <= 0xefU && length >= 3) {
        const UInt32 second = continuation(1);
        const UInt32 third  = continuation(2);
        if (second != 0xffffffffU && third != 0xffffffffU) {
            const UInt32 value = ((first & 0x0fU) << 12U) | (second << 6U) | third;
            if (value >= 0x800U && !(value >= 0xd800U && value <= 0xdfffU)) {
                used = 3;
                return value;
            }
        }
    } else if (first >= 0xf0U && first <= 0xf4U && length >= 4) {
        const UInt32 second = continuation(1);
        const UInt32 third  = continuation(2);
        const UInt32 fourth = continuation(3);
        if (second != 0xffffffffU && third != 0xffffffffU && fourth != 0xffffffffU) {
            const UInt32 value =
                ((first & 0x07U) << 18U) | (second << 12U) | (third << 6U) | fourth;
            if (value >= 0x10000U && value <= 0x10ffffU) {
                used = 4;
                return value;
            }
        }
    }
    used = 1;
    return 0xfffdU;
}

std::wstring utf8_to_wide(const char* data, size_t length) {
    std::wstring result;
    for (size_t index = 0; index < length;) {
        size_t       used       = 0;
        const UInt32 code_point = decode_utf8_code_point(data + index, length - index, used);
        index += used == 0 ? 1 : used;
        if constexpr (sizeof(wchar_t) == 2) {
            if (code_point <= 0xffffU) {
                result.push_back(static_cast<wchar_t>(code_point));
            } else {
                const UInt32 adjusted = code_point - 0x10000U;
                result.push_back(static_cast<wchar_t>(0xd800U + (adjusted >> 10U)));
                result.push_back(static_cast<wchar_t>(0xdc00U + (adjusted & 0x3ffU)));
            }
        } else {
            result.push_back(static_cast<wchar_t>(code_point));
        }
    }
    return result;
}

std::string wide_to_utf8(const wchar_t* data) {
    std::string result;
    if (data == nullptr) {
        return result;
    }
    const size_t length = std::wcslen(data);
    for (size_t index = 0; index < length; ++index) {
        UInt32 code_point = static_cast<UInt32>(data[index]);
        if constexpr (sizeof(wchar_t) == 2) {
            if (code_point >= 0xd800U && code_point <= 0xdbffU && index + 1 < length) {
                const UInt32 low = static_cast<UInt16>(data[index + 1]);
                if (low >= 0xdc00U && low <= 0xdfffU) {
                    code_point = 0x10000U + ((code_point - 0xd800U) << 10U) + (low - 0xdc00U);
                    ++index;
                }
            }
        }
        append_utf8(result, code_point);
    }
    return result;
}

bool prop_bool(const PROPVARIANT& value, bool& result) {
    if (value.vt == VT_BOOL) {
        result = value.boolVal != VARIANT_FALSE;
        return true;
    }
    return false;
}

bool prop_u32(const PROPVARIANT& value, UInt32& result) {
    if (value.vt == VT_UI4) {
        result = value.ulVal;
        return true;
    }
    if (value.vt == VT_UI8 && value.uhVal.QuadPart <= std::numeric_limits<UInt32>::max()) {
        result = static_cast<UInt32>(value.uhVal.QuadPart);
        return true;
    }
    return false;
}

bool prop_u64(const PROPVARIANT& value, UInt64& result) {
    if (value.vt == VT_UI8) {
        result = value.uhVal.QuadPart;
        return true;
    }
    if (value.vt == VT_UI4) {
        result = value.ulVal;
        return true;
    }
    if (value.vt == VT_I8 && value.hVal.QuadPart >= 0) {
        result = static_cast<UInt64>(value.hVal.QuadPart);
        return true;
    }
    return false;
}

bool prop_string(const PROPVARIANT& value, std::string& result) {
    if (value.vt != VT_BSTR) {
        return false;
    }
    result = bstr_to_utf8(value.bstrVal);
    return true;
}

bool prop_binary_guid(const PROPVARIANT& value, uint8_t result[16]) {
    if (value.vt != VT_BSTR || SysStringByteLen(value.bstrVal) < 16U) {
        return false;
    }
    std::memcpy(result, value.bstrVal, 16U);
    return true;
}

HRESULT handler_property(const Pulp7zBridge* bridge,
                         UInt32              index,
                         PROPID              property_id,
                         PROPVARIANT&        value) {
    value    = {};
    value.vt = VT_EMPTY;
    return bridge->get_handler_property2(index, property_id, &value);
}

HRESULT method_property(const Pulp7zBridge* bridge,
                        UInt32              index,
                        PROPID              property_id,
                        PROPVARIANT&        value) {
    value    = {};
    value.vt = VT_EMPTY;
    return bridge->get_method_property(index, property_id, &value);
}

struct EntryData {
    Pulp7zEntryInfo info{};
    std::string     path;
    std::string     method;
    std::string     link_target;

    void sync() {
        info.path            = path.data();
        info.path_len        = static_cast<UInt32>(path.size());
        info.method          = method.empty() ? nullptr : method.data();
        info.method_len      = static_cast<UInt32>(method.size());
        info.link_target     = link_target.empty() ? nullptr : link_target.data();
        info.link_target_len = static_cast<UInt32>(link_target.size());
    }
};

constexpr uint8_t kLinkNone   = 0;
constexpr uint8_t kLinkSymbol = 1;
constexpr uint8_t kLinkHard   = 2;

Int64 filetime_to_unix_ns(const FILETIME& file_time) {
    constexpr Int64 kUnixEpochInFileTime = 11644473600LL * 10000000LL;
    const UInt64    value =
        (static_cast<UInt64>(file_time.dwHighDateTime) << 32U) | file_time.dwLowDateTime;
    const __int128 unix_100ns = static_cast<__int128>(value) - kUnixEpochInFileTime;
    const __int128 unix_ns    = unix_100ns * 100;
    if (unix_ns > std::numeric_limits<Int64>::max()) {
        return std::numeric_limits<Int64>::max();
    }
    if (unix_ns < std::numeric_limits<Int64>::min()) {
        return std::numeric_limits<Int64>::min();
    }
    return static_cast<Int64>(unix_ns);
}

FILETIME unix_ns_to_filetime(Int64 unix_ns) {
    constexpr Int64 kUnixEpochInFileTime = 11644473600LL * 10000000LL;
    const Int64     value                = unix_ns / 100 + kUnixEpochInFileTime;
    FILETIME        result{};
    result.dwLowDateTime  = static_cast<UInt32>(static_cast<UInt64>(value));
    result.dwHighDateTime = static_cast<UInt32>(static_cast<UInt64>(value) >> 32U);
    return result;
}

HRESULT read_archive_property(IInArchive*  archive,
                              UInt32       index,
                              PROPID       property_id,
                              PROPVARIANT& value) {
    value    = {};
    value.vt = VT_EMPTY;
    return archive->GetProperty(index, property_id, &value);
}

HRESULT fill_entry(IInArchive* archive, UInt32 index, EntryData& entry) {
    entry            = {};
    entry.info.index = index;

    PROPVARIANT value{};
    HRESULT     result = read_archive_property(archive, index, kpidPath, value);
    if (failed(result)) {
        return result;
    }
    if (!prop_string(value, entry.path) || entry.path.empty()) {
        entry.path = "[Content]";
    }
    VariantClear(&value);

    result = read_archive_property(archive, index, kpidIsDir, value);
    if (failed(result)) {
        VariantClear(&value);
        return result;
    }
    bool boolean_value = false;
    entry.info.is_dir  = prop_bool(value, boolean_value) && boolean_value;
    VariantClear(&value);

    result = read_archive_property(archive, index, kpidEncrypted, value);
    if (failed(result)) {
        VariantClear(&value);
        return result;
    }
    entry.info.encrypted = prop_bool(value, boolean_value) && boolean_value;
    VariantClear(&value);

    result = read_archive_property(archive, index, kpidSize, value);
    if (failed(result)) {
        VariantClear(&value);
        return result;
    }
    entry.info.has_size = prop_u64(value, entry.info.size);
    VariantClear(&value);

    result = read_archive_property(archive, index, kpidPackSize, value);
    if (failed(result)) {
        VariantClear(&value);
        return result;
    }
    entry.info.has_pack_size = prop_u64(value, entry.info.pack_size);
    VariantClear(&value);

    result = read_archive_property(archive, index, kpidMTime, value);
    if (failed(result)) {
        VariantClear(&value);
        return result;
    }
    if (value.vt == VT_FILETIME) {
        entry.info.has_mtime     = 1;
        entry.info.mtime_unix_ns = filetime_to_unix_ns(value.filetime);
    }
    VariantClear(&value);

    result = read_archive_property(archive, index, kpidAttrib, value);
    if (failed(result)) {
        VariantClear(&value);
        return result;
    }
    entry.info.has_attrib = prop_u32(value, entry.info.attrib);
    VariantClear(&value);

    result = read_archive_property(archive, index, kpidPosixAttrib, value);
    if (!failed(result)) {
        entry.info.has_posix_attrib = prop_u32(value, entry.info.posix_attrib);
    }
    VariantClear(&value);

    result = read_archive_property(archive, index, kpidCRC, value);
    if (failed(result)) {
        VariantClear(&value);
        return result;
    }
    entry.info.has_crc = prop_u32(value, entry.info.crc);
    VariantClear(&value);

    result = read_archive_property(archive, index, kpidMethod, value);
    if (failed(result)) {
        VariantClear(&value);
        return result;
    }
    prop_string(value, entry.method);
    VariantClear(&value);

    result = read_archive_property(archive, index, kpidSymLink, value);
    if (failed(result)) {
        VariantClear(&value);
        return result;
    }
    if (prop_string(value, entry.link_target) && !entry.link_target.empty()) {
        entry.info.link_kind = kLinkSymbol;
    }
    VariantClear(&value);
    if (entry.link_target.empty()) {
        result = read_archive_property(archive, index, kpidHardLink, value);
        if (failed(result)) {
            VariantClear(&value);
            return result;
        }
        if (prop_string(value, entry.link_target) && !entry.link_target.empty()) {
            entry.info.link_kind = kLinkHard;
        }
        VariantClear(&value);
    }
    if (entry.link_target.empty()) {
        entry.info.link_kind = kLinkNone;
    }
    entry.sync();
    return S_OK;
}

void copy_entry_info(const Pulp7zEntryInfo& source, EntryData& destination) {
    destination      = {};
    destination.info = source;
    if (source.path != nullptr) {
        destination.path.assign(source.path, source.path_len);
    }
    if (source.method != nullptr) {
        destination.method.assign(source.method, source.method_len);
    }
    if (source.link_target != nullptr) {
        destination.link_target.assign(source.link_target, source.link_target_len);
    }
    destination.sync();
}

class PulpInStream final : public IInStream, public CMyUnknownImp {
    Z7_IFACES_IMP_UNK_1(IInStream)
    Z7_IFACE_COM7_IMP(ISequentialInStream)

public:
    PulpInStream(const Pulp7zInputCallbacks* callbacks, int32_t& callback_status)
        : _callbacks(callbacks),
          _callback_status(callback_status) {
    }

private:
    const Pulp7zInputCallbacks* _callbacks;
    int32_t&                    _callback_status;
};

Z7_COM7F_IMF(PulpInStream::Read(void* data, UInt32 size, UInt32* processed)) {
    if (processed == nullptr || (_callbacks == nullptr && size != 0)) {
        return E_INVALIDARG;
    }
    *processed = 0;
    if (size == 0) {
        return S_OK;
    }
    UInt32        actual = 0;
    const int32_t status =
        _callbacks->read(_callbacks->user, static_cast<uint8_t*>(data), size, &actual);
    if (status != PULP7Z_OK) {
        _callback_status = status;
        return E_ABORT;
    }
    *processed = actual <= size ? actual : 0;
    return actual <= size ? S_OK : E_FAIL;
}

Z7_COM7F_IMF(PulpInStream::Seek(Int64 offset, UInt32 origin, UInt64* position)) {
    if (_callbacks == nullptr || _callbacks->seek == nullptr) {
        return E_INVALIDARG;
    }
    UInt64        ignored_position = 0;
    const int32_t status           = _callbacks->seek(
        _callbacks->user, offset, origin, position == nullptr ? &ignored_position : position);
    if (status != PULP7Z_OK) {
        _callback_status = status;
        return E_ABORT;
    }
    return S_OK;
}

class PulpVolumeStream final : public IInStream, public CMyUnknownImp {
    Z7_IFACES_IMP_UNK_1(IInStream)
    Z7_IFACE_COM7_IMP(ISequentialInStream)

public:
    PulpVolumeStream(Pulp7zVolumeCallbacks callbacks, int32_t& callback_status)
        : _callbacks(callbacks),
          _callback_status(callback_status) {
    }

    ~PulpVolumeStream() {
        if (_callbacks.close != nullptr && _callbacks.user != nullptr) {
            _callbacks.close(_callbacks.user);
        }
    }

private:
    Pulp7zVolumeCallbacks _callbacks;
    int32_t&              _callback_status;
};

Z7_COM7F_IMF(PulpVolumeStream::Read(void* data, UInt32 size, UInt32* processed)) {
    if (processed == nullptr || (_callbacks.read == nullptr && size != 0)) {
        return E_INVALIDARG;
    }
    *processed = 0;
    if (size == 0) {
        return S_OK;
    }
    UInt32        actual = 0;
    const int32_t status =
        _callbacks.read(_callbacks.user, static_cast<uint8_t*>(data), size, &actual);
    if (status != PULP7Z_OK) {
        _callback_status = status;
        return E_ABORT;
    }
    if (actual > size) {
        return E_FAIL;
    }
    *processed = actual;
    return S_OK;
}

Z7_COM7F_IMF(PulpVolumeStream::Seek(Int64 offset, UInt32 origin, UInt64* position)) {
    if (_callbacks.seek == nullptr) {
        return E_NOTIMPL;
    }
    UInt64        ignored_position = 0;
    const int32_t status           = _callbacks.seek(
        _callbacks.user, offset, origin, position == nullptr ? &ignored_position : position);
    if (status != PULP7Z_OK) {
        _callback_status = status;
        return E_ABORT;
    }
    return S_OK;
}

class PulpSourceInStream final : public ISequentialInStream, public CMyUnknownImp {
    Z7_IFACES_IMP_UNK_1(ISequentialInStream)

public:
    PulpSourceInStream(const Pulp7zSourceCallbacks* callbacks,
                       UInt32                       index,
                       int32_t&                     callback_status)
        : _callbacks(callbacks),
          _index(index),
          _callback_status(callback_status) {
    }

private:
    const Pulp7zSourceCallbacks* _callbacks;
    UInt32                       _index;
    int32_t&                     _callback_status;
};

Z7_COM7F_IMF(PulpSourceInStream::Read(void* data, UInt32 size, UInt32* processed)) {
    if (processed == nullptr || (_callbacks == nullptr && size != 0)) {
        return E_INVALIDARG;
    }
    *processed = 0;
    if (size == 0) {
        return S_OK;
    }
    UInt32        actual = 0;
    const int32_t status =
        _callbacks->read(_callbacks->user, _index, static_cast<uint8_t*>(data), size, &actual);
    if (status != PULP7Z_OK) {
        _callback_status = status;
        return E_ABORT;
    }
    *processed = actual <= size ? actual : 0;
    return actual <= size ? S_OK : E_FAIL;
}

class PulpOutputStream final : public IOutStream, public CMyUnknownImp {
    Z7_IFACES_IMP_UNK_1(IOutStream)
    Z7_IFACE_COM7_IMP(ISequentialOutStream)

public:
    PulpOutputStream(const Pulp7zOutputCallbacks* callbacks, int32_t& callback_status)
        : _callbacks(callbacks),
          _callback_status(callback_status) {
    }

    PulpOutputStream(Pulp7zWriteCallback callback, void* user, int32_t& callback_status)
        : _callback_status(callback_status) {
        _owned_callbacks.user  = user;
        _owned_callbacks.write = callback;
        _owned_callbacks.seek  = nullptr;
        _callbacks             = &_owned_callbacks;
    }

    UInt64 bytes() const {
        return _bytes;
    }

private:
    const Pulp7zOutputCallbacks* _callbacks = nullptr;
    Pulp7zOutputCallbacks        _owned_callbacks{};
    int32_t&                     _callback_status;
    UInt64                       _bytes = 0;
};

Z7_COM7F_IMF(PulpOutputStream::Write(const void* data, UInt32 size, UInt32* processed)) {
    if (processed == nullptr || (_callbacks == nullptr && size != 0) ||
        (_callbacks != nullptr && _callbacks->write == nullptr && size != 0)) {
        return E_INVALIDARG;
    }
    *processed = 0;
    if (size == 0) {
        return S_OK;
    }
    UInt32        actual = 0;
    const int32_t status =
        _callbacks->write(_callbacks->user, static_cast<const uint8_t*>(data), size, &actual);
    if (status != PULP7Z_OK) {
        _callback_status = status;
        return E_ABORT;
    }
    if (actual > size || actual == 0) {
        return E_FAIL;
    }
    *processed = actual;
    if (_bytes > std::numeric_limits<UInt64>::max() - actual) {
        return E_FAIL;
    }
    _bytes += actual;
    return S_OK;
}

Z7_COM7F_IMF(PulpOutputStream::Seek(Int64 offset, UInt32 origin, UInt64* position)) {
    if (_callbacks == nullptr || _callbacks->seek == nullptr) {
        return E_NOTIMPL;
    }
    UInt64        ignored_position = 0;
    const int32_t status           = _callbacks->seek(
        _callbacks->user, offset, origin, position == nullptr ? &ignored_position : position);
    if (status != PULP7Z_OK) {
        _callback_status = status;
        return E_ABORT;
    }
    return S_OK;
}

Z7_COM7F_IMF(PulpOutputStream::SetSize(UInt64 /* size */)) {
    return E_NOTIMPL;
}

class PulpOpenCallback final : public IArchiveOpenCallback,
                               public IArchiveOpenVolumeCallback,
                               public ICryptoGetTextPassword,
                               public CMyUnknownImp {
    Z7_IFACES_IMP_UNK_3(
        IArchiveOpenCallback, IArchiveOpenVolumeCallback, ICryptoGetTextPassword)

public:
    PulpOpenCallback(void*                  user,
                     Pulp7zProgressCallback progress,
                     Pulp7zPasswordCallback password,
                     Pulp7zOpenVolumeCallback volume,
                     const char*            archive_name,
                     UInt32                 archive_name_len,
                     int32_t&               callback_status)
        : _user(user),
          _progress(progress),
          _password(password),
          _volume(volume),
          _archive_name(utf8_to_wide(
              archive_name == nullptr ? "" : archive_name,
              archive_name == nullptr ? 0 : archive_name_len)),
          _callback_status(callback_status) {
    }

    UInt64 total() const {
        return _total;
    }

private:
    void*                  _user;
    Pulp7zProgressCallback _progress;
    Pulp7zPasswordCallback _password;
    Pulp7zOpenVolumeCallback _volume;
    std::wstring            _archive_name;
    int32_t&               _callback_status;
    UInt64                 _total            = 0;
    UInt32                 _password_attempt = 0;
};

Z7_COM7F_IMF(PulpOpenCallback::GetProperty(PROPID propID, PROPVARIANT* value)) {
    if (value == nullptr) {
        return E_INVALIDARG;
    }
    *value = {};
    value->vt = VT_EMPTY;
    if (propID == kpidName && !_archive_name.empty()) {
        value->bstrVal = SysAllocStringLen(
            _archive_name.data(), static_cast<UINT>(_archive_name.size()));
        if (value->bstrVal == nullptr) {
            return E_OUTOFMEMORY;
        }
        value->vt = VT_BSTR;
    }
    return S_OK;
}

Z7_COM7F_IMF(PulpOpenCallback::GetStream(const wchar_t* name, IInStream** in_stream)) {
    if (in_stream == nullptr) {
        return E_INVALIDARG;
    }
    *in_stream = nullptr;
    if (_volume == nullptr || name == nullptr) {
        return S_FALSE;
    }
    const std::string requested_name = wide_to_utf8(name);
    Pulp7zVolumeCallbacks callbacks{};
    const int32_t status = _volume(
        _user, requested_name.data(), static_cast<UInt32>(requested_name.size()), &callbacks);
    if (status == PULP7Z_STREAM_UNAVAILABLE) {
        return S_FALSE;
    }
    if (status != PULP7Z_OK) {
        _callback_status = status;
        return E_ABORT;
    }
    if (callbacks.user == nullptr || callbacks.read == nullptr || callbacks.seek == nullptr) {
        if (callbacks.close != nullptr && callbacks.user != nullptr) {
            callbacks.close(callbacks.user);
        }
        _callback_status = PULP7Z_CALLBACK_ERROR;
        return E_ABORT;
    }
    auto* stream = new PulpVolumeStream(callbacks, _callback_status);
    // COM out-parameters transfer one owned reference to the caller.  The
    // 7-Zip SDK stores this pointer in CMyComPtr, so establish that reference
    // before detaching it from the temporary smart pointer.
    CMyComPtr<IInStream> owned_stream(stream);
    *in_stream = owned_stream.Detach();
    return S_OK;
}

Z7_COM7F_IMF(PulpOpenCallback::SetTotal(const UInt64* files, const UInt64* bytes)) {
    (void)files;
    _total = bytes == nullptr ? 0 : *bytes;
    if (_progress == nullptr) {
        return S_OK;
    }
    const int32_t status = _progress(_user, _total, 0, 0);
    if (status != PULP7Z_OK) {
        _callback_status = status;
        return E_ABORT;
    }
    return S_OK;
}

Z7_COM7F_IMF(PulpOpenCallback::SetCompleted(const UInt64* files, const UInt64* bytes)) {
    (void)files;
    if (_progress == nullptr) {
        return S_OK;
    }
    const int32_t status = _progress(_user, _total, bytes == nullptr ? 0 : *bytes, 0);
    if (status != PULP7Z_OK) {
        _callback_status = status;
        return E_ABORT;
    }
    return S_OK;
}

static HRESULT fill_password(Pulp7zPasswordCallback password_callback,
                             const void*            user,
                             UInt32                 reason,
                             UInt32&                attempt,
                             int32_t&               callback_status,
                             BSTR*                  password) {
    if (password == nullptr) {
        return E_INVALIDARG;
    }
    *password = nullptr;
    if (password_callback == nullptr) {
        callback_status = PULP7Z_PASSWORD_DECLINED;
        return E_ABORT;
    }
    ++attempt;
    std::vector<uint8_t> buffer(32768);
    UInt32               length = 0;
    const int32_t        status = password_callback(const_cast<void*>(user),
                                                    reason,
                                                    attempt,
                                                    buffer.data(),
                                                    static_cast<UInt32>(buffer.size()),
                                                    &length);
    if (status != PULP7Z_OK) {
        callback_status = status;
        return E_ABORT;
    }
    if (length == 0) {
        callback_status = PULP7Z_PASSWORD_DECLINED;
        return E_ABORT;
    }
    if (length > buffer.size()) {
        callback_status = PULP7Z_CALLBACK_ERROR;
        return E_FAIL;
    }
    const std::wstring wide = utf8_to_wide(reinterpret_cast<const char*>(buffer.data()), length);
    *password               = SysAllocStringLen(wide.data(), static_cast<UINT>(wide.size()));
    std::fill(buffer.begin(), buffer.end(), 0);
    return *password == nullptr ? E_OUTOFMEMORY : S_OK;
}

Z7_COM7F_IMF(PulpOpenCallback::CryptoGetTextPassword(BSTR* password)) {
    return fill_password(_password, _user, 0, _password_attempt, _callback_status, password);
}

class PulpExtractCallback final : public IArchiveExtractCallback,
                                  public ICryptoGetTextPassword,
                                  public CMyUnknownImp {
    Z7_IFACES_IMP_UNK_2(IArchiveExtractCallback, ICryptoGetTextPassword)
    Z7_IFACE_COM7_IMP(IProgress)

public:
    PulpExtractCallback(IInArchive*                   archive,
                        const Pulp7zExtractCallbacks* callbacks,
                        int32_t&                      callback_status)
        : _archive(archive),
          _callbacks(callbacks),
          _callback_status(callback_status) {
    }

private:
    IInArchive*                     _archive;
    const Pulp7zExtractCallbacks*   _callbacks;
    int32_t&                        _callback_status;
    EntryData                       _current;
    bool                            _active      = false;
    UInt32                          _decision    = kDecisionSkip;
    PulpOutputStream*               _output_spec = nullptr;
    CMyComPtr<ISequentialOutStream> _output;
    UInt32                          _password_attempt = 0;
    UInt64                          _total            = 0;
};

Z7_COM7F_IMF(PulpExtractCallback::SetTotal(UInt64 total)) {
    _total = total;
    if (_callbacks == nullptr || _callbacks->progress == nullptr) {
        return S_OK;
    }
    const int32_t status = _callbacks->progress(_callbacks->user, total, 0, 1);
    if (status != PULP7Z_OK) {
        _callback_status = status;
        return E_ABORT;
    }
    return S_OK;
}

Z7_COM7F_IMF(PulpExtractCallback::SetCompleted(const UInt64* complete_value)) {
    if (_callbacks == nullptr || _callbacks->progress == nullptr) {
        return S_OK;
    }
    const int32_t status = _callbacks->progress(
        _callbacks->user, _total, complete_value == nullptr ? 0 : *complete_value, 1);
    if (status != PULP7Z_OK) {
        _callback_status = status;
        return E_ABORT;
    }
    return S_OK;
}

Z7_COM7F_IMF(PulpExtractCallback::GetStream(UInt32                 index,
                                            ISequentialOutStream** out_stream,
                                            Int32                  ask_extract_mode)) {
    if (out_stream == nullptr || _callbacks == nullptr || _callbacks->begin == nullptr) {
        return E_INVALIDARG;
    }
    *out_stream = nullptr;
    _output.Release();
    _output_spec = nullptr;
    _active      = false;

    HRESULT result = fill_entry(_archive, index, _current);
    if (failed(result)) {
        return result;
    }

    UInt32        decision = kDecisionSkip;
    const int32_t status   = _callbacks->begin(
        _callbacks->user, &_current.info, static_cast<UInt32>(ask_extract_mode), &decision);
    if (status != PULP7Z_OK) {
        _callback_status = status;
        return E_ABORT;
    }
    if (decision > kDecisionWrite) {
        _callback_status = PULP7Z_CALLBACK_ERROR;
        return E_INVALIDARG;
    }
    _decision = decision;
    _active   = true;

    if (ask_extract_mode == static_cast<Int32>(kAskExtract) && decision == kDecisionWrite) {
        auto* stream = new PulpOutputStream(_callbacks->write, _callbacks->user, _callback_status);
        _output      = stream;
        static_cast<ISequentialOutStream*>(stream)->AddRef();
        _output_spec = stream;
        *out_stream  = stream;
    }
    return S_OK;
}

Z7_COM7F_IMF(PulpExtractCallback::PrepareOperation(Int32 ask_extract_mode)) {
    (void)ask_extract_mode;
    return S_OK;
}

Z7_COM7F_IMF(PulpExtractCallback::SetOperationResult(Int32 operation_result)) {
    if (!_active) {
        return S_OK;
    }
    const UInt64 bytes = _output_spec == nullptr ? 0 : _output_spec->bytes();
    _active            = false;
    if (_callbacks == nullptr || _callbacks->finish == nullptr) {
        _output.Release();
        _output_spec = nullptr;
        return E_INVALIDARG;
    }
    const int32_t status =
        _callbacks->finish(_callbacks->user, &_current.info, operation_result, bytes);
    _output.Release();
    _output_spec = nullptr;
    if (status != PULP7Z_OK) {
        _callback_status = status;
        return E_ABORT;
    }
    return S_OK;
}

Z7_COM7F_IMF(PulpExtractCallback::CryptoGetTextPassword(BSTR* password)) {
    if (_callbacks == nullptr) {
        return E_INVALIDARG;
    }
    return fill_password(
        _callbacks->password, _callbacks->user, 1, _password_attempt, _callback_status, password);
}

class PulpListCallback {
public:
    PulpListCallback(Pulp7zEntryCallback callback, void* user, int32_t& callback_status)
        : _callback(callback),
          _user(user),
          _callback_status(callback_status) {
    }

    bool visit(const EntryData& entry) {
        if (_callback == nullptr) {
            _callback_status = PULP7Z_INVALID_ARGUMENT;
            return false;
        }
        const int32_t status = _callback(_user, &entry.info);
        if (status != PULP7Z_OK) {
            _callback_status = status;
            return false;
        }
        return true;
    }

private:
    Pulp7zEntryCallback _callback;
    void*               _user;
    int32_t&            _callback_status;
};

class PulpUpdateCallback final : public IArchiveUpdateCallback,
                                 public ICryptoGetTextPassword2,
                                 public CMyUnknownImp {
    Z7_IFACES_IMP_UNK_2(IArchiveUpdateCallback, ICryptoGetTextPassword2)
    Z7_IFACE_COM7_IMP(IProgress)

public:
    PulpUpdateCallback(const Pulp7zSourceCallbacks* source, int32_t& callback_status)
        : _source(source),
          _callback_status(callback_status) {
    }

    const char* stage() const {
        return _stage;
    }

private:
    const Pulp7zSourceCallbacks* _source;
    int32_t&                     _callback_status;
    UInt64                       _total            = 0;
    UInt32                       _password_attempt = 0;
    const char*                  _stage            = "before update callback";
};

bool get_source_entry(const Pulp7zSourceCallbacks* source,
                      UInt32                       index,
                      EntryData&                   entry,
                      int32_t&                     callback_status) {
    if (source == nullptr || source->entry == nullptr || index >= source->count) {
        callback_status = PULP7Z_INVALID_ARGUMENT;
        return false;
    }
    Pulp7zEntryInfo info{};
    const int32_t   status = source->entry(source->user, index, &info);
    if (status != PULP7Z_OK) {
        callback_status = status;
        return false;
    }
    copy_entry_info(info, entry);
    entry.info.index = index;
    return true;
}

Z7_COM7F_IMF(PulpUpdateCallback::SetTotal(UInt64 total)) {
    _stage = "SetTotal";
    _total = total;
    if (_source == nullptr || _source->progress == nullptr) {
        return S_OK;
    }
    const int32_t status = _source->progress(_source->user, total, 0, 2);
    if (status != PULP7Z_OK) {
        _callback_status = status;
        return E_ABORT;
    }
    return S_OK;
}

Z7_COM7F_IMF(PulpUpdateCallback::SetCompleted(const UInt64* complete_value)) {
    _stage = "SetCompleted";
    if (_source == nullptr || _source->progress == nullptr) {
        return S_OK;
    }
    const int32_t status = _source->progress(
        _source->user, _total, complete_value == nullptr ? 0 : *complete_value, 2);
    if (status != PULP7Z_OK) {
        _callback_status = status;
        return E_ABORT;
    }
    return S_OK;
}

Z7_COM7F_IMF(PulpUpdateCallback::GetUpdateItemInfo(
    UInt32 index, Int32* new_data, Int32* new_properties, UInt32* index_in_archive)) {
    _stage = "GetUpdateItemInfo";
    if (_source == nullptr || index >= _source->count) {
        _callback_status = PULP7Z_INVALID_ARGUMENT;
        return E_INVALIDARG;
    }
    if (new_data != nullptr) {
        EntryData entry;
        if (!get_source_entry(_source, index, entry, _callback_status)) {
            return E_ABORT;
        }
        // A newly supplied directory is still new data.  The 7-Zip update
        // protocol uses indexInArchive only when newData is false; returning
        // false for a new directory would make handlers index an archive
        // entry with -1.
        *new_data = 1;
    }
    if (new_properties != nullptr) {
        *new_properties = 1;
    }
    if (index_in_archive != nullptr) {
        *index_in_archive = static_cast<UInt32>(-1);
    }
    return S_OK;
}

void set_empty_property(PROPVARIANT* value) {
    if (value != nullptr) {
        VariantClear(value);
        value->vt = VT_EMPTY;
    }
}

HRESULT set_string_property(PROPVARIANT* value, const std::string& text) {
    if (value == nullptr) {
        return E_INVALIDARG;
    }
    const std::wstring wide   = utf8_to_wide(text.data(), text.size());
    BSTR               result = SysAllocStringLen(wide.data(), static_cast<UINT>(wide.size()));
    if (result == nullptr && !wide.empty()) {
        return E_OUTOFMEMORY;
    }
    value->vt      = VT_BSTR;
    value->bstrVal = result;
    return S_OK;
}

Z7_COM7F_IMF(PulpUpdateCallback::GetProperty(UInt32       index,
                                             PROPID       property_id,
                                             PROPVARIANT* value)) {
    _stage = "GetProperty";
    if (value == nullptr) {
        return E_INVALIDARG;
    }
    set_empty_property(value);
    EntryData entry;
    if (!get_source_entry(_source, index, entry, _callback_status)) {
        return E_ABORT;
    }
    switch (property_id) {
    case kpidPath:
        _stage = "GetProperty:path";
        return set_string_property(value, entry.path);
    case kpidIsDir:
        _stage         = "GetProperty:is-dir";
        value->vt      = VT_BOOL;
        value->boolVal = entry.info.is_dir ? VARIANT_TRUE : VARIANT_FALSE;
        return S_OK;
    case kpidSize:
        _stage = "GetProperty:size";
        if (entry.info.has_size || entry.info.is_dir) {
            value->vt             = VT_UI8;
            value->uhVal.QuadPart = entry.info.is_dir ? 0 : entry.info.size;
        }
        return S_OK;
    case kpidMTime:
        _stage = "GetProperty:mtime";
        if (entry.info.has_mtime) {
            value->vt       = VT_FILETIME;
            value->filetime = unix_ns_to_filetime(entry.info.mtime_unix_ns);
        }
        return S_OK;
    case kpidAttrib:
        _stage = "GetProperty:attrib";
        if (entry.info.has_attrib) {
            value->vt    = VT_UI4;
            value->ulVal = entry.info.attrib;
        }
        return S_OK;
    case kpidPosixAttrib:
        _stage = "GetProperty:posix-attrib";
        if (entry.info.has_posix_attrib) {
            value->vt    = VT_UI4;
            value->ulVal = entry.info.posix_attrib;
        }
        return S_OK;
    case kpidIsAnti:
        _stage         = "GetProperty:is-anti";
        value->vt      = VT_BOOL;
        value->boolVal = VARIANT_FALSE;
        return S_OK;
    default:
        return S_OK;
    }
}

Z7_COM7F_IMF(PulpUpdateCallback::GetStream(UInt32 index, ISequentialInStream** in_stream)) {
    _stage = "GetStream";
    if (in_stream == nullptr || _source == nullptr || _source->read == nullptr) {
        return E_INVALIDARG;
    }
    *in_stream = nullptr;
    EntryData entry;
    if (!get_source_entry(_source, index, entry, _callback_status)) {
        return E_ABORT;
    }
    if (entry.info.is_dir) {
        return S_OK;
    }
    auto* stream = new PulpSourceInStream(_source, index, _callback_status);
    static_cast<ISequentialInStream*>(stream)->AddRef();
    *in_stream = stream;
    return S_OK;
}

Z7_COM7F_IMF(PulpUpdateCallback::SetOperationResult(Int32 operation_result)) {
    _stage = "SetOperationResult";
    if (operation_result != NArchive::NUpdate::NOperationResult::kOK) {
        _callback_status = operation_result;
        return E_ABORT;
    }
    return S_OK;
}

Z7_COM7F_IMF(PulpUpdateCallback::CryptoGetTextPassword2(Int32* password_is_defined,
                                                        BSTR*  password)) {
    _stage = "CryptoGetTextPassword2";
    if (password_is_defined == nullptr || password == nullptr || _source == nullptr) {
        return E_INVALIDARG;
    }
    *password_is_defined = 0;
    *password            = nullptr;
    const HRESULT result = fill_password(
        _source->password, _source->user, 2, _password_attempt, _callback_status, password);
    if (result == S_OK) {
        *password_is_defined = 1;
        return S_OK;
    }
    if (_callback_status == PULP7Z_PASSWORD_DECLINED) {
        _callback_status = PULP7Z_OK;
        return S_OK;
    }
    return E_ABORT;
}

static HRESULT read_handler_prop(const Pulp7zBridge* bridge,
                                 UInt32              index,
                                 PROPID              property_id,
                                 PROPVARIANT&        value) {
    return handler_property(bridge, index, property_id, value);
}

static HRESULT read_method_prop(const Pulp7zBridge* bridge,
                                UInt32              index,
                                PROPID              property_id,
                                PROPVARIANT&        value) {
    return method_property(bridge, index, property_id, value);
}

static int32_t enumerate_formats_impl(const Pulp7zBridge*  bridge,
                                      Pulp7zFormatCallback callback,
                                      void*                user,
                                      Pulp7zError*         error) {
    UInt32  count  = 0;
    HRESULT result = bridge->get_number_of_formats(&count);
    if (failed(result)) {
        return set_hresult_error(error, result, "GetNumberOfFormats failed");
    }
    for (UInt32 index = 0; index < count; ++index) {
        Pulp7zFormatInfo info{};
        info.index = index;
        std::string          name;
        std::string          extension;
        std::string          add_extension;
        std::vector<uint8_t> signature;
        std::vector<uint8_t> multi_signature;
        PROPVARIANT          value{};

        result = read_handler_prop(bridge, index, NArchive::NHandlerPropID::kName, value);
        if (failed(result)) {
            return set_hresult_error(error, result, "GetHandlerProperty2(name) failed");
        }
        prop_string(value, name);
        VariantClear(&value);

        result = read_handler_prop(bridge, index, NArchive::NHandlerPropID::kExtension, value);
        if (failed(result)) {
            return set_hresult_error(error, result, "GetHandlerProperty2(extension) failed");
        }
        prop_string(value, extension);
        VariantClear(&value);

        result = read_handler_prop(bridge, index, NArchive::NHandlerPropID::kAddExtension, value);
        if (failed(result)) {
            return set_hresult_error(error, result, "GetHandlerProperty2(add-extension) failed");
        }
        prop_string(value, add_extension);
        VariantClear(&value);

        result = read_handler_prop(bridge, index, NArchive::NHandlerPropID::kClassID, value);
        if (failed(result)) {
            return set_hresult_error(error, result, "GetHandlerProperty2(class-id) failed");
        }
        info.has_class_id = prop_binary_guid(value, info.class_id) ? 1 : 0;
        VariantClear(&value);

        result = read_handler_prop(bridge, index, NArchive::NHandlerPropID::kUpdate, value);
        if (failed(result)) {
            return set_hresult_error(error, result, "GetHandlerProperty2(update) failed");
        }
        bool bool_value = false;
        info.update     = prop_bool(value, bool_value) && bool_value;
        VariantClear(&value);

        result = read_handler_prop(bridge, index, NArchive::NHandlerPropID::kKeepName, value);
        if (failed(result)) {
            return set_hresult_error(error, result, "GetHandlerProperty2(keep-name) failed");
        }
        info.keep_name = prop_bool(value, bool_value) && bool_value;
        VariantClear(&value);

        result = read_handler_prop(bridge, index, NArchive::NHandlerPropID::kAltStreams, value);
        if (failed(result)) {
            return set_hresult_error(error, result, "GetHandlerProperty2(alt-streams) failed");
        }
        info.alt_streams = prop_bool(value, bool_value) && bool_value;
        VariantClear(&value);

        result = read_handler_prop(bridge, index, NArchive::NHandlerPropID::kNtSecure, value);
        if (failed(result)) {
            return set_hresult_error(error, result, "GetHandlerProperty2(nt-secure) failed");
        }
        info.nt_secure = prop_bool(value, bool_value) && bool_value;
        VariantClear(&value);

        result = read_handler_prop(bridge, index, NArchive::NHandlerPropID::kFlags, value);
        if (failed(result)) {
            return set_hresult_error(error, result, "GetHandlerProperty2(flags) failed");
        }
        prop_u32(value, info.flags);
        VariantClear(&value);

        result = read_handler_prop(bridge, index, NArchive::NHandlerPropID::kTimeFlags, value);
        if (failed(result)) {
            return set_hresult_error(error, result, "GetHandlerProperty2(time-flags) failed");
        }
        prop_u32(value, info.time_flags);
        VariantClear(&value);

        result =
            read_handler_prop(bridge, index, NArchive::NHandlerPropID::kSignatureOffset, value);
        if (failed(result)) {
            return set_hresult_error(error, result, "GetHandlerProperty2(signature-offset) failed");
        }
        prop_u32(value, info.signature_offset);
        VariantClear(&value);

        result = read_handler_prop(bridge, index, NArchive::NHandlerPropID::kSignature, value);
        if (failed(result)) {
            return set_hresult_error(error, result, "GetHandlerProperty2(signature) failed");
        }
        if (value.vt == VT_BSTR && value.bstrVal != nullptr) {
            const UINT length = SysStringByteLen(value.bstrVal);
            signature.assign(reinterpret_cast<const uint8_t*>(value.bstrVal),
                             reinterpret_cast<const uint8_t*>(value.bstrVal) + length);
        }
        VariantClear(&value);

        result = read_handler_prop(bridge, index, NArchive::NHandlerPropID::kMultiSignature, value);
        if (failed(result)) {
            return set_hresult_error(error, result, "GetHandlerProperty2(multi-signature) failed");
        }
        if (value.vt == VT_BSTR && value.bstrVal != nullptr) {
            const UINT length = SysStringByteLen(value.bstrVal);
            multi_signature.assign(reinterpret_cast<const uint8_t*>(value.bstrVal),
                                   reinterpret_cast<const uint8_t*>(value.bstrVal) + length);
        }
        VariantClear(&value);

        info.name                = name.data();
        info.name_len            = static_cast<UInt32>(name.size());
        info.extension           = extension.data();
        info.extension_len       = static_cast<UInt32>(extension.size());
        info.add_extension       = add_extension.data();
        info.add_extension_len   = static_cast<UInt32>(add_extension.size());
        info.signature           = signature.empty() ? nullptr : signature.data();
        info.signature_len       = static_cast<UInt32>(signature.size());
        info.multi_signature     = multi_signature.empty() ? nullptr : multi_signature.data();
        info.multi_signature_len = static_cast<UInt32>(multi_signature.size());

        const int32_t callback_status = callback(user, &info);
        if (callback_status != PULP7Z_OK) {
            return set_error(error, callback_status, "format callback failed");
        }
    }
    return PULP7Z_OK;
}

static int32_t enumerate_methods_impl(const Pulp7zBridge*  bridge,
                                      Pulp7zMethodCallback callback,
                                      void*                user,
                                      Pulp7zError*         error) {
    UInt32  count  = 0;
    HRESULT result = bridge->get_number_of_methods(&count);
    if (failed(result)) {
        return set_hresult_error(error, result, "GetNumberOfMethods failed");
    }
    for (UInt32 index = 0; index < count; ++index) {
        Pulp7zMethodInfo info{};
        info.index = index;
        std::string name;
        std::string description;
        PROPVARIANT value{};

        result = read_method_prop(bridge, index, NMethodPropID::kID, value);
        if (failed(result)) {
            return set_hresult_error(error, result, "GetMethodProperty(id) failed");
        }
        info.has_id = prop_binary_guid(value, info.id) ? 1 : 0;
        VariantClear(&value);

        result = read_method_prop(bridge, index, NMethodPropID::kName, value);
        if (failed(result)) {
            return set_hresult_error(error, result, "GetMethodProperty(name) failed");
        }
        prop_string(value, name);
        VariantClear(&value);

        result = read_method_prop(bridge, index, NMethodPropID::kDescription, value);
        if (failed(result)) {
            return set_hresult_error(error, result, "GetMethodProperty(description) failed");
        }
        prop_string(value, description);
        VariantClear(&value);

        result = read_method_prop(bridge, index, NMethodPropID::kDecoder, value);
        if (failed(result)) {
            return set_hresult_error(error, result, "GetMethodProperty(decoder) failed");
        }
        info.has_decoder = prop_binary_guid(value, info.decoder) ? 1 : 0;
        VariantClear(&value);

        result = read_method_prop(bridge, index, NMethodPropID::kEncoder, value);
        if (failed(result)) {
            return set_hresult_error(error, result, "GetMethodProperty(encoder) failed");
        }
        info.has_encoder = prop_binary_guid(value, info.encoder) ? 1 : 0;
        VariantClear(&value);

        bool bool_value = false;
        result          = read_method_prop(bridge, index, NMethodPropID::kDecoderIsAssigned, value);
        if (failed(result)) {
            return set_hresult_error(error, result, "GetMethodProperty(decoder-assigned) failed");
        }
        info.decoder_assigned = prop_bool(value, bool_value) && bool_value;
        VariantClear(&value);

        result = read_method_prop(bridge, index, NMethodPropID::kEncoderIsAssigned, value);
        if (failed(result)) {
            return set_hresult_error(error, result, "GetMethodProperty(encoder-assigned) failed");
        }
        info.encoder_assigned = prop_bool(value, bool_value) && bool_value;
        VariantClear(&value);

        result = read_method_prop(bridge, index, NMethodPropID::kIsFilter, value);
        if (failed(result)) {
            return set_hresult_error(error, result, "GetMethodProperty(filter) failed");
        }
        info.is_filter = prop_bool(value, bool_value) && bool_value;
        VariantClear(&value);

        info.name                     = name.data();
        info.name_len                 = static_cast<UInt32>(name.size());
        info.description              = description.data();
        info.description_len          = static_cast<UInt32>(description.size());
        const int32_t callback_status = callback(user, &info);
        if (callback_status != PULP7Z_OK) {
            return set_error(error, callback_status, "method callback failed");
        }
    }
    return PULP7Z_OK;
}

static bool class_id_from_bytes(const uint8_t bytes[16], GUID& result) {
    if (bytes == nullptr) {
        return false;
    }
    std::memcpy(&result, bytes, sizeof(result));
    return true;
}

static HRESULT set_update_properties(IOutArchive* archive, const Pulp7zUpdateOptions* options) {
    if (archive == nullptr || options == nullptr) {
        return E_INVALIDARG;
    }
    const bool   has_method            = options->method != nullptr && options->method_len != 0;
    const bool   has_level             = options->level >= 0;
    const bool   has_solid             = options->solid >= 0;
    const bool   has_header_encryption = options->header_encryption >= 0;
    const UInt32 count = static_cast<UInt32>(has_method) + static_cast<UInt32>(has_level) +
                         static_cast<UInt32>(has_solid) +
                         static_cast<UInt32>(has_header_encryption);
    if (count == 0) {
        return S_OK;
    }

    CMyComPtr<ISetProperties> properties;
    HRESULT                   result =
        archive->QueryInterface(IID_ISetProperties, reinterpret_cast<void**>(&properties));
    if (failed(result) || !properties) {
        return result == S_OK ? E_NOINTERFACE : result;
    }

    std::wstring   method;
    const wchar_t* names[4]{};
    PROPVARIANT    values[4]{};
    UInt32         index = 0;
    if (has_method) {
        method                = utf8_to_wide(options->method, options->method_len);
        names[index]          = L"m";
        values[index].vt      = VT_BSTR;
        values[index].bstrVal = SysAllocStringLen(method.data(), static_cast<UINT>(method.size()));
        if (values[index].bstrVal == nullptr) {
            for (UInt32 i = 0; i < index; ++i) {
                VariantClear(&values[i]);
            }
            return E_OUTOFMEMORY;
        }
        ++index;
    }
    if (has_level) {
        names[index]        = L"x";
        values[index].vt    = VT_UI4;
        values[index].ulVal = static_cast<UInt32>(options->level);
        ++index;
    }
    if (has_solid) {
        names[index]          = L"s";
        values[index].vt      = VT_BOOL;
        values[index].boolVal = options->solid ? VARIANT_TRUE : VARIANT_FALSE;
        ++index;
    }
    if (has_header_encryption) {
        names[index]          = L"he";
        values[index].vt      = VT_BOOL;
        values[index].boolVal = options->header_encryption ? VARIANT_TRUE : VARIANT_FALSE;
        ++index;
    }

    result = properties->SetProperties(names, values, count);
    for (UInt32 i = 0; i < count; ++i) {
        VariantClear(&values[i]);
    }
    return result;
}

} // namespace

extern "C" int32_t pulp7z_bridge_create(Pulp7zCreateObjectFn        create_object,
                                        Pulp7zGetNumberOfFormatsFn  get_number_of_formats,
                                        Pulp7zGetHandlerPropertyFn  get_handler_property,
                                        Pulp7zGetHandlerProperty2Fn get_handler_property2,
                                        Pulp7zGetNumberOfMethodsFn  get_number_of_methods,
                                        Pulp7zGetMethodPropertyFn   get_method_property,
                                        Pulp7zBridge**              out_bridge,
                                        Pulp7zError*                out_error) {
    clear_error(out_error);
    if (out_bridge == nullptr || create_object == nullptr || get_number_of_formats == nullptr ||
        get_handler_property == nullptr || get_handler_property2 == nullptr ||
        get_number_of_methods == nullptr || get_method_property == nullptr) {
        return set_error(out_error, PULP7Z_INVALID_ARGUMENT, "missing Format7z function pointer");
    }
    *out_bridge = nullptr;
    try {
        *out_bridge = new Pulp7zBridge{
            create_object,
            get_number_of_formats,
            get_handler_property,
            get_handler_property2,
            get_number_of_methods,
            get_method_property,
        };
        return PULP7Z_OK;
    } catch (const std::exception&) {
        return set_error(out_error, PULP7Z_NATIVE_ERROR, "bridge allocation failed");
    } catch (...) {
        return set_error(out_error, PULP7Z_NATIVE_ERROR, "bridge allocation failed");
    }
}

extern "C" void pulp7z_bridge_destroy(Pulp7zBridge* bridge) {
    delete bridge;
}

extern "C" int32_t pulp7z_bridge_enumerate_formats(Pulp7zBridge*        bridge,
                                                   Pulp7zFormatCallback callback,
                                                   void*                user,
                                                   Pulp7zError*         out_error) {
    clear_error(out_error);
    if (bridge == nullptr || callback == nullptr) {
        return set_error(
            out_error, PULP7Z_INVALID_ARGUMENT, "invalid format enumeration arguments");
    }
    try {
        return enumerate_formats_impl(bridge, callback, user, out_error);
    } catch (const std::exception&) {
        return set_error(out_error, PULP7Z_NATIVE_ERROR, "C++ exception in format enumeration");
    } catch (...) {
        return set_error(out_error, PULP7Z_NATIVE_ERROR, "unknown exception in format enumeration");
    }
}

extern "C" int32_t pulp7z_bridge_enumerate_methods(Pulp7zBridge*        bridge,
                                                   Pulp7zMethodCallback callback,
                                                   void*                user,
                                                   Pulp7zError*         out_error) {
    clear_error(out_error);
    if (bridge == nullptr || callback == nullptr) {
        return set_error(
            out_error, PULP7Z_INVALID_ARGUMENT, "invalid method enumeration arguments");
    }
    try {
        return enumerate_methods_impl(bridge, callback, user, out_error);
    } catch (const std::exception&) {
        return set_error(out_error, PULP7Z_NATIVE_ERROR, "C++ exception in method enumeration");
    } catch (...) {
        return set_error(out_error, PULP7Z_NATIVE_ERROR, "unknown exception in method enumeration");
    }
}

extern "C" int32_t pulp7z_bridge_list(Pulp7zBridge*               bridge,
                                      const uint8_t               class_id[16],
                                      const Pulp7zInputCallbacks* input,
                                      const Pulp7zOpenCallbacks*  open_callbacks,
                                      Pulp7zEntryCallback         callback,
                                      void*                       user,
                                      Pulp7zError*                out_error) {
    clear_error(out_error);
    if (bridge == nullptr || class_id == nullptr || input == nullptr || input->read == nullptr ||
        input->seek == nullptr || callback == nullptr) {
        return set_error(out_error, PULP7Z_INVALID_ARGUMENT, "invalid list arguments");
    }
    try {
        GUID class_guid{};
        class_id_from_bytes(class_id, class_guid);
        CMyComPtr<IInArchive> archive;
        HRESULT               result =
            bridge->create_object(&class_guid, &IID_IInArchive, reinterpret_cast<void**>(&archive));
        if (failed(result) || !archive) {
            return set_hresult_error(out_error, result, "cannot create input archive handler");
        }
        int32_t              callback_status = PULP7Z_OK;
        auto*                input_spec      = new PulpInStream(input, callback_status);
        CMyComPtr<IInStream> input_stream(input_spec);
        auto*                open_spec =
            new PulpOpenCallback(open_callbacks == nullptr ? nullptr : open_callbacks->user,
                                 open_callbacks == nullptr ? nullptr : open_callbacks->progress,
                                 open_callbacks == nullptr ? nullptr : open_callbacks->password,
                                 open_callbacks == nullptr ? nullptr : open_callbacks->volume,
                                 open_callbacks == nullptr ? nullptr : open_callbacks->archive_name,
                                 open_callbacks == nullptr ? 0 : open_callbacks->archive_name_len,
                                 callback_status);
        CMyComPtr<IArchiveOpenCallback> open_callback(open_spec);
        const UInt64                    scan_size = 1U << 23;
        result = archive->Open(input_stream, &scan_size, open_callback);
        if (result != S_OK) {
            archive->Close();
            if (callback_status != PULP7Z_OK) {
                return set_error(out_error, callback_status, "Rust callback failed while opening");
            }
            return set_hresult_error(out_error, result, "Format7z could not open the archive");
        }
        UInt32 count = 0;
        result       = archive->GetNumberOfItems(&count);
        if (failed(result)) {
            archive->Close();
            return set_hresult_error(out_error, result, "GetNumberOfItems failed");
        }
        PulpListCallback visitor(callback, user, callback_status);
        for (UInt32 index = 0; index < count; ++index) {
            EntryData entry;
            result = fill_entry(archive, index, entry);
            if (failed(result) || !visitor.visit(entry)) {
                break;
            }
        }
        const HRESULT close_result = archive->Close();
        if (callback_status != PULP7Z_OK) {
            return set_error(out_error, callback_status, "Rust callback failed while listing");
        }
        if (failed(result)) {
            return set_hresult_error(out_error, result, "GetProperty failed while listing");
        }
        if (failed(close_result)) {
            return set_hresult_error(out_error, close_result, "archive close failed");
        }
        return PULP7Z_OK;
    } catch (const std::exception&) {
        return set_error(out_error, PULP7Z_NATIVE_ERROR, "C++ exception while listing");
    } catch (...) {
        return set_error(out_error, PULP7Z_NATIVE_ERROR, "unknown exception while listing");
    }
}

extern "C" int32_t pulp7z_bridge_probe(Pulp7zBridge*               bridge,
                                       const uint8_t               class_id[16],
                                       const Pulp7zInputCallbacks* input,
                                       const Pulp7zOpenCallbacks*  open_callbacks,
                                       Pulp7zError*                out_error) {
    clear_error(out_error);
    if (bridge == nullptr || class_id == nullptr || input == nullptr || input->read == nullptr ||
        input->seek == nullptr) {
        return set_error(out_error, PULP7Z_INVALID_ARGUMENT, "invalid probe arguments");
    }
    try {
        GUID class_guid{};
        class_id_from_bytes(class_id, class_guid);
        CMyComPtr<IInArchive> archive;
        HRESULT               result =
            bridge->create_object(&class_guid, &IID_IInArchive, reinterpret_cast<void**>(&archive));
        if (failed(result) || !archive) {
            return set_hresult_error(out_error, result, "cannot create input archive handler");
        }
        int32_t              callback_status = PULP7Z_OK;
        auto*                input_spec      = new PulpInStream(input, callback_status);
        CMyComPtr<IInStream> input_stream(input_spec);
        auto*                open_spec =
            new PulpOpenCallback(open_callbacks == nullptr ? nullptr : open_callbacks->user,
                                 open_callbacks == nullptr ? nullptr : open_callbacks->progress,
                                 open_callbacks == nullptr ? nullptr : open_callbacks->password,
                                 open_callbacks == nullptr ? nullptr : open_callbacks->volume,
                                 open_callbacks == nullptr ? nullptr : open_callbacks->archive_name,
                                 open_callbacks == nullptr ? 0 : open_callbacks->archive_name_len,
                                 callback_status);
        CMyComPtr<IArchiveOpenCallback> open_callback(open_spec);
        const UInt64                    scan_size = 1U << 23;
        result = archive->Open(input_stream, &scan_size, open_callback);
        if (result != S_OK) {
            archive->Close();
            if (callback_status != PULP7Z_OK) {
                return set_error(out_error, callback_status, "Rust callback failed while probing");
            }
            return set_hresult_error(out_error, result, "Format7z could not open the archive");
        }
        const HRESULT close_result = archive->Close();
        if (callback_status != PULP7Z_OK) {
            return set_error(out_error, callback_status, "Rust callback failed while probing");
        }
        if (failed(close_result)) {
            return set_hresult_error(out_error, close_result, "archive close failed");
        }
        return PULP7Z_OK;
    } catch (const std::exception&) {
        return set_error(out_error, PULP7Z_NATIVE_ERROR, "C++ exception while probing");
    } catch (...) {
        return set_error(out_error, PULP7Z_NATIVE_ERROR, "unknown exception while probing");
    }
}

extern "C" int32_t pulp7z_bridge_copy_entry(Pulp7zBridge*                bridge,
                                             const uint8_t                class_id[16],
                                             const Pulp7zInputCallbacks*  input,
                                             const Pulp7zOpenCallbacks*   open_callbacks,
                                             uint32_t                     index,
                                             const Pulp7zOutputCallbacks* output,
                                             Pulp7zError*                  out_error) {
    clear_error(out_error);
    if (bridge == nullptr || class_id == nullptr || input == nullptr || input->read == nullptr ||
        input->seek == nullptr || output == nullptr || output->write == nullptr) {
        return set_error(out_error, PULP7Z_INVALID_ARGUMENT, "invalid copy-entry arguments");
    }
    try {
        GUID class_guid{};
        class_id_from_bytes(class_id, class_guid);
        CMyComPtr<IInArchive> archive;
        HRESULT               result =
            bridge->create_object(&class_guid, &IID_IInArchive, reinterpret_cast<void**>(&archive));
        if (failed(result) || !archive) {
            return set_hresult_error(out_error, result, "cannot create input archive handler");
        }

        int32_t              callback_status = PULP7Z_OK;
        auto*                input_spec      = new PulpInStream(input, callback_status);
        CMyComPtr<IInStream> input_stream(input_spec);
        auto*                open_spec =
            new PulpOpenCallback(open_callbacks == nullptr ? nullptr : open_callbacks->user,
                                 open_callbacks == nullptr ? nullptr : open_callbacks->progress,
                                 open_callbacks == nullptr ? nullptr : open_callbacks->password,
                                 open_callbacks == nullptr ? nullptr : open_callbacks->volume,
                                 open_callbacks == nullptr ? nullptr : open_callbacks->archive_name,
                                 open_callbacks == nullptr ? 0 : open_callbacks->archive_name_len,
                                 callback_status);
        CMyComPtr<IArchiveOpenCallback> open_callback(open_spec);
        const UInt64                    scan_size = 1U << 23;
        result = archive->Open(input_stream, &scan_size, open_callback);
        if (result != S_OK) {
            archive->Close();
            if (callback_status != PULP7Z_OK) {
                return set_error(
                    out_error, callback_status, "Rust callback failed while opening");
            }
            return set_hresult_error(out_error, result, "Format7z could not open the archive");
        }

        CMyComPtr<IInArchiveGetStream> get_stream;
        result = archive->QueryInterface(
            IID_IInArchiveGetStream, reinterpret_cast<void**>(&get_stream));
        if (failed(result) || !get_stream) {
            const HRESULT close_result = archive->Close();
            if (callback_status != PULP7Z_OK) {
                return set_error(
                    out_error, callback_status, "Rust callback failed while opening the archive");
            }
            if (failed(close_result)) {
                return set_hresult_error(out_error, close_result, "archive close failed");
            }
            return PULP7Z_STREAM_UNAVAILABLE;
        }

        CMyComPtr<ISequentialInStream> child_stream;
        result = get_stream->GetStream(index, &child_stream);
        if (failed(result) || !child_stream) {
            get_stream.Release();
            child_stream.Release();
            const HRESULT close_result = archive->Close();
            if (callback_status != PULP7Z_OK) {
                return set_error(
                    out_error, callback_status, "Rust callback failed while opening the entry");
            }
            if (failed(close_result)) {
                return set_hresult_error(out_error, close_result, "archive close failed");
            }
            return PULP7Z_STREAM_UNAVAILABLE;
        }

        PulpOutputStream output_spec(output, callback_status);
        auto*            output_stream = static_cast<ISequentialOutStream*>(&output_spec);
        std::vector<uint8_t> buffer(1U << 20);
        UInt64                completed = 0;
        while (true) {
            UInt32 read = 0;
            result       = child_stream->Read(buffer.data(), static_cast<UInt32>(buffer.size()), &read);
            if (failed(result)) {
                break;
            }
            if (callback_status != PULP7Z_OK) {
                result = E_ABORT;
                break;
            }
            if (read == 0) {
                break;
            }

            UInt32 written = 0;
            result          = output_stream->Write(buffer.data(), read, &written);
            if (failed(result) || written != read) {
                if (!failed(result)) {
                    result = E_FAIL;
                }
                break;
            }
            if (completed > std::numeric_limits<UInt64>::max() - written) {
                result = E_FAIL;
                break;
            }
            completed += written;
            if (open_callbacks != nullptr && open_callbacks->progress != nullptr) {
                const int32_t status =
                    open_callbacks->progress(open_callbacks->user, 0, completed, 1);
                if (status != PULP7Z_OK) {
                    callback_status = status;
                    result          = E_ABORT;
                    break;
                }
            }
        }

        child_stream.Release();
        get_stream.Release();
        const HRESULT close_result = archive->Close();
        if (callback_status != PULP7Z_OK) {
            return set_error(out_error, callback_status, "Rust callback failed while copying");
        }
        if (failed(result)) {
            return set_hresult_error(out_error, result, "Format7z entry stream copy failed");
        }
        if (failed(close_result)) {
            return set_hresult_error(out_error, close_result, "archive close failed");
        }
        return PULP7Z_OK;
    } catch (const std::exception&) {
        return set_error(out_error, PULP7Z_NATIVE_ERROR, "C++ exception while copying entry");
    } catch (...) {
        return set_error(out_error, PULP7Z_NATIVE_ERROR, "unknown exception while copying entry");
    }
}

extern "C" int32_t pulp7z_bridge_extract(Pulp7zBridge*                 bridge,
                                         const uint8_t                 class_id[16],
                                         const Pulp7zInputCallbacks*   input,
                                         const uint32_t*               indices,
                                         uint32_t                      index_count,
                                         int32_t                       test_mode,
                                         const Pulp7zExtractCallbacks* callbacks,
                                         Pulp7zError*                  out_error) {
    clear_error(out_error);
    if (bridge == nullptr || class_id == nullptr || input == nullptr || input->read == nullptr ||
        input->seek == nullptr || callbacks == nullptr || callbacks->begin == nullptr ||
        callbacks->finish == nullptr || (index_count != 0 && indices == nullptr)) {
        return set_error(out_error, PULP7Z_INVALID_ARGUMENT, "invalid extract arguments");
    }
    try {
        GUID class_guid{};
        class_id_from_bytes(class_id, class_guid);
        CMyComPtr<IInArchive> archive;
        HRESULT               result =
            bridge->create_object(&class_guid, &IID_IInArchive, reinterpret_cast<void**>(&archive));
        if (failed(result) || !archive) {
            return set_hresult_error(out_error, result, "cannot create input archive handler");
        }
        int32_t              callback_status = PULP7Z_OK;
        auto*                input_spec      = new PulpInStream(input, callback_status);
        CMyComPtr<IInStream> input_stream(input_spec);
        auto*                open_spec = new PulpOpenCallback(
            callbacks->user,
            callbacks->progress,
            callbacks->password,
            callbacks->volume,
            callbacks->archive_name,
            callbacks->archive_name_len,
            callback_status);
        CMyComPtr<IArchiveOpenCallback> open_callback(open_spec);
        const UInt64                    scan_size = 1U << 23;
        result = archive->Open(input_stream, &scan_size, open_callback);
        if (result != S_OK) {
            archive->Close();
            if (callback_status != PULP7Z_OK) {
                return set_error(out_error, callback_status, "Rust callback failed while opening");
            }
            return set_hresult_error(out_error, result, "Format7z could not open the archive");
        }

        auto* extract_spec = new PulpExtractCallback(archive, callbacks, callback_status);
        CMyComPtr<IArchiveExtractCallback> extract_callback(extract_spec);
        result = archive->Extract(indices,
                                  index_count == 0 ? static_cast<UInt32>(-1) : index_count,
                                  test_mode,
                                  extract_callback);
        const HRESULT close_result = archive->Close();
        if (callback_status != PULP7Z_OK) {
            return set_error(out_error, callback_status, "Rust callback failed while extracting");
        }
        if (failed(result)) {
            return set_hresult_error(out_error, result, "Format7z extraction failed");
        }
        if (failed(close_result)) {
            return set_hresult_error(out_error, close_result, "archive close failed");
        }
        return PULP7Z_OK;
    } catch (const std::exception&) {
        return set_error(out_error, PULP7Z_NATIVE_ERROR, "C++ exception while extracting");
    } catch (...) {
        return set_error(out_error, PULP7Z_NATIVE_ERROR, "unknown exception while extracting");
    }
}

extern "C" int32_t pulp7z_bridge_update(Pulp7zBridge*                bridge,
                                        const uint8_t                class_id[16],
                                        const Pulp7zOutputCallbacks* output,
                                        const Pulp7zSourceCallbacks* source,
                                        const Pulp7zUpdateOptions*   options,
                                        Pulp7zError*                 out_error) {
    clear_error(out_error);
    if (bridge == nullptr || class_id == nullptr || output == nullptr || output->write == nullptr ||
        source == nullptr || source->entry == nullptr || source->read == nullptr ||
        options == nullptr) {
        return set_error(out_error, PULP7Z_INVALID_ARGUMENT, "invalid update arguments");
    }
    try {
        GUID class_guid{};
        class_id_from_bytes(class_id, class_guid);
        CMyComPtr<IOutArchive> archive;
        HRESULT                result = bridge->create_object(
            &class_guid, &IID_IOutArchive, reinterpret_cast<void**>(&archive));
        if (failed(result) || !archive) {
            return set_hresult_error(out_error, result, "cannot create output archive handler");
        }

        result = set_update_properties(archive, options);
        if (failed(result)) {
            return set_hresult_error(out_error, result, "SetProperties failed");
        }

        int32_t                         callback_status = PULP7Z_OK;
        auto*                           output_spec = new PulpOutputStream(output, callback_status);
        CMyComPtr<ISequentialOutStream> output_stream(output_spec);
        auto* update_spec = new PulpUpdateCallback(source, callback_status);
        CMyComPtr<IArchiveUpdateCallback> update_callback(update_spec);
        result = archive->UpdateItems(output_stream, source->count, update_callback);
        if (callback_status != PULP7Z_OK) {
            char message[128]{};
            std::snprintf(message,
                          sizeof(message),
                          "Rust callback failed while updating (%s)",
                          update_spec->stage());
            return set_error(out_error, callback_status, message);
        }
        if (failed(result)) {
            char message[128]{};
            std::snprintf(
                message, sizeof(message), "Format7z update failed after %s", update_spec->stage());
            return set_hresult_error(out_error, result, message);
        }
        return PULP7Z_OK;
    } catch (const std::exception&) {
        return set_error(out_error, PULP7Z_NATIVE_ERROR, "C++ exception while updating");
    } catch (...) {
        return set_error(out_error, PULP7Z_NATIVE_ERROR, "unknown exception while updating");
    }
}
