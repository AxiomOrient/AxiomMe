import Foundation

public enum AxiommeClientError: Error {
    case invalidArgument(String)
    case runtime(String)
    case invalidState(String)
    case decode(String)
}

/// Minimal Swift wrapper around `axiomme_mobile_ffi`.
///
/// Ownership model:
/// - Rust allocates `payload` bytes for each call.
/// - Swift copies payload into `Data` and immediately calls `axiomme_owned_bytes_free`.
/// - The opaque runtime handle is imported as `OpaquePointer` from C.
public final class AxiommeClient {
    private var runtime: OpaquePointer?

    public init(rootDir: String) throws {
        var outRuntime: OpaquePointer?
        let result = rootDir.withCString { rawRoot in
            axiomme_runtime_new(rawRoot, &outRuntime)
        }
        let payload = takePayload(result.payload)
        try throwIfError(result.code, payload: payload)

        guard let runtime = outRuntime else {
            throw AxiommeClientError.invalidState("runtime pointer was not initialized")
        }
        self.runtime = runtime
    }

    deinit {
        if let runtime {
            axiomme_runtime_free(runtime)
        }
    }

    public func initialize() throws {
        guard let runtime else {
            throw AxiommeClientError.invalidState("runtime is already released")
        }
        let result = axiomme_runtime_initialize(runtime)
        let payload = takePayload(result.payload)
        try throwIfError(result.code, payload: payload)
    }

    public func backendStatus() throws -> [String: Any] {
        guard let runtime else {
            throw AxiommeClientError.invalidState("runtime is already released")
        }
        let result = axiomme_runtime_backend_status_json(runtime)
        let payload = takePayload(result.payload)
        try throwIfError(result.code, payload: payload)

        guard !payload.isEmpty else {
            throw AxiommeClientError.decode("backend status payload was empty")
        }
        let json = try JSONSerialization.jsonObject(with: payload, options: [])
        guard let dict = json as? [String: Any] else {
            throw AxiommeClientError.decode("backend status payload was not a JSON object")
        }
        return dict
    }
}

private func takePayload(_ payload: AxiommeOwnedBytes) -> Data {
    guard let base = payload.ptr, payload.len > 0 else {
        axiomme_owned_bytes_free(payload)
        return Data()
    }
    let count = Int(payload.len)
    let data = Data(bytes: base, count: count)
    axiomme_owned_bytes_free(payload)
    return data
}

private func throwIfError(_ code: AxiommeFfiCode, payload: Data) throws {
    if code == AXIOMME_FFI_CODE_OK {
        return
    }

    let message = parseErrorMessage(payload) ?? "unknown ffi error"
    switch code {
    case Int32(AXIOMME_FFI_CODE_INVALID_ARGUMENT):
        throw AxiommeClientError.invalidArgument(message)
    case Int32(AXIOMME_FFI_CODE_RUNTIME_ERROR):
        throw AxiommeClientError.runtime(message)
    default:
        throw AxiommeClientError.runtime(message)
    }
}

private func parseErrorMessage(_ payload: Data) -> String? {
    guard !payload.isEmpty else {
        return nil
    }
    if let json = try? JSONSerialization.jsonObject(with: payload, options: []),
       let dict = json as? [String: Any],
       let message = dict["message"] as? String,
       !message.isEmpty {
        return message
    }
    let text = String(data: payload, encoding: .utf8)?.trimmingCharacters(in: .whitespacesAndNewlines)
    return text?.isEmpty == false ? text : nil
}
