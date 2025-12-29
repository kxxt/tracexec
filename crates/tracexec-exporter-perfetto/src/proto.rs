/*
    Original copyright license of perfetto trace protobuf definitions:
                                 Apache License
                           Version 2.0, January 2004
                        http://www.apache.org/licenses/

   TERMS AND CONDITIONS FOR USE, REPRODUCTION, AND DISTRIBUTION

   1. Definitions.

      "License" shall mean the terms and conditions for use, reproduction,
      and distribution as defined by Sections 1 through 9 of this document.

      "Licensor" shall mean the copyright owner or entity authorized by
      the copyright owner that is granting the License.

      "Legal Entity" shall mean the union of the acting entity and all
      other entities that control, are controlled by, or are under common
      control with that entity. For the purposes of this definition,
      "control" means (i) the power, direct or indirect, to cause the
      direction or management of such entity, whether by contract or
      otherwise, or (ii) ownership of fifty percent (50%) or more of the
      outstanding shares, or (iii) beneficial ownership of such entity.

      "You" (or "Your") shall mean an individual or Legal Entity
      exercising permissions granted by this License.

      "Source" form shall mean the preferred form for making modifications,
      including but not limited to software source code, documentation
      source, and configuration files.

      "Object" form shall mean any form resulting from mechanical
      transformation or translation of a Source form, including but
      not limited to compiled object code, generated documentation,
      and conversions to other media types.

      "Work" shall mean the work of authorship, whether in Source or
      Object form, made available under the License, as indicated by a
      copyright notice that is included in or attached to the work
      (an example is provided in the Appendix below).

      "Derivative Works" shall mean any work, whether in Source or Object
      form, that is based on (or derived from) the Work and for which the
      editorial revisions, annotations, elaborations, or other modifications
      represent, as a whole, an original work of authorship. For the purposes
      of this License, Derivative Works shall not include works that remain
      separable from, or merely link (or bind by name) to the interfaces of,
      the Work and Derivative Works thereof.

      "Contribution" shall mean any work of authorship, including
      the original version of the Work and any modifications or additions
      to that Work or Derivative Works thereof, that is intentionally
      submitted to Licensor for inclusion in the Work by the copyright owner
      or by an individual or Legal Entity authorized to submit on behalf of
      the copyright owner. For the purposes of this definition, "submitted"
      means any form of electronic, verbal, or written communication sent
      to the Licensor or its representatives, including but not limited to
      communication on electronic mailing lists, source code control systems,
      and issue tracking systems that are managed by, or on behalf of, the
      Licensor for the purpose of discussing and improving the Work, but
      excluding communication that is conspicuously marked or otherwise
      designated in writing by the copyright owner as "Not a Contribution."

      "Contributor" shall mean Licensor and any individual or Legal Entity
      on behalf of whom a Contribution has been received by Licensor and
      subsequently incorporated within the Work.

   2. Grant of Copyright License. Subject to the terms and conditions of
      this License, each Contributor hereby grants to You a perpetual,
      worldwide, non-exclusive, no-charge, royalty-free, irrevocable
      copyright license to reproduce, prepare Derivative Works of,
      publicly display, publicly perform, sublicense, and distribute the
      Work and such Derivative Works in Source or Object form.

   3. Grant of Patent License. Subject to the terms and conditions of
      this License, each Contributor hereby grants to You a perpetual,
      worldwide, non-exclusive, no-charge, royalty-free, irrevocable
      (except as stated in this section) patent license to make, have made,
      use, offer to sell, sell, import, and otherwise transfer the Work,
      where such license applies only to those patent claims licensable
      by such Contributor that are necessarily infringed by their
      Contribution(s) alone or by combination of their Contribution(s)
      with the Work to which such Contribution(s) was submitted. If You
      institute patent litigation against any entity (including a
      cross-claim or counterclaim in a lawsuit) alleging that the Work
      or a Contribution incorporated within the Work constitutes direct
      or contributory patent infringement, then any patent licenses
      granted to You under this License for that Work shall terminate
      as of the date such litigation is filed.

   4. Redistribution. You may reproduce and distribute copies of the
      Work or Derivative Works thereof in any medium, with or without
      modifications, and in Source or Object form, provided that You
      meet the following conditions:

      (a) You must give any other recipients of the Work or
          Derivative Works a copy of this License; and

      (b) You must cause any modified files to carry prominent notices
          stating that You changed the files; and

      (c) You must retain, in the Source form of any Derivative Works
          that You distribute, all copyright, patent, trademark, and
          attribution notices from the Source form of the Work,
          excluding those notices that do not pertain to any part of
          the Derivative Works; and

      (d) If the Work includes a "NOTICE" text file as part of its
          distribution, then any Derivative Works that You distribute must
          include a readable copy of the attribution notices contained
          within such NOTICE file, excluding those notices that do not
          pertain to any part of the Derivative Works, in at least one
          of the following places: within a NOTICE text file distributed
          as part of the Derivative Works; within the Source form or
          documentation, if provided along with the Derivative Works; or,
          within a display generated by the Derivative Works, if and
          wherever such third-party notices normally appear. The contents
          of the NOTICE file are for informational purposes only and
          do not modify the License. You may add Your own attribution
          notices within Derivative Works that You distribute, alongside
          or as an addendum to the NOTICE text from the Work, provided
          that such additional attribution notices cannot be construed
          as modifying the License.

      You may add Your own copyright statement to Your modifications and
      may provide additional or different license terms and conditions
      for use, reproduction, or distribution of Your modifications, or
      for any such Derivative Works as a whole, provided Your use,
      reproduction, and distribution of the Work otherwise complies with
      the conditions stated in this License.

   5. Submission of Contributions. Unless You explicitly state otherwise,
      any Contribution intentionally submitted for inclusion in the Work
      by You to the Licensor shall be under the terms and conditions of
      this License, without any additional terms or conditions.
      Notwithstanding the above, nothing herein shall supersede or modify
      the terms of any separate license agreement you may have executed
      with Licensor regarding such Contributions.

   6. Trademarks. This License does not grant permission to use the trade
      names, trademarks, service marks, or product names of the Licensor,
      except as required for reasonable and customary use in describing the
      origin of the Work and reproducing the content of the NOTICE file.

   7. Disclaimer of Warranty. Unless required by applicable law or
      agreed to in writing, Licensor provides the Work (and each
      Contributor provides its Contributions) on an "AS IS" BASIS,
      WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or
      implied, including, without limitation, any warranties or conditions
      of TITLE, NON-INFRINGEMENT, MERCHANTABILITY, or FITNESS FOR A
      PARTICULAR PURPOSE. You are solely responsible for determining the
      appropriateness of using or redistributing the Work and assume any
      risks associated with Your exercise of permissions under this License.

   8. Limitation of Liability. In no event and under no legal theory,
      whether in tort (including negligence), contract, or otherwise,
      unless required by applicable law (such as deliberate and grossly
      negligent acts) or agreed to in writing, shall any Contributor be
      liable to You for damages, including any direct, indirect, special,
      incidental, or consequential damages of any character arising as a
      result of this License or out of the use or inability to use the
      Work (including but not limited to damages for loss of goodwill,
      work stoppage, computer failure or malfunction, or any and all
      other commercial damages or losses), even if such Contributor
      has been advised of the possibility of such damages.

   9. Accepting Warranty or Additional Liability. While redistributing
      the Work or Derivative Works thereof, You may choose to offer,
      and charge a fee for, acceptance of support, warranty, indemnity,
      or other liability obligations and/or rights consistent with this
      License. However, in accepting such obligations, You may act only
      on Your own behalf and on Your sole responsibility, not on behalf
      of any other Contributor, and only if You agree to indemnify,
      defend, and hold each Contributor harmless for any liability
      incurred by, or claims asserted against, such Contributor by reason
      of your accepting any such warranty or additional liability.

   END OF TERMS AND CONDITIONS

------------------

Files: * except those files noted below

   Copyright (c) 2017, The Android Open Source Project

   Licensed under the Apache License, Version 2.0 (the "License");
   you may not use this file except in compliance with the License.

   Unless required by applicable law or agreed to in writing, software
   distributed under the License is distributed on an "AS IS" BASIS,
   WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
   See the License for the specific language governing permissions and
   limitations under the License.
*/

#[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
pub struct Utsname {
  #[prost(string, optional, tag = "1")]
  pub sysname: ::core::option::Option<::prost::alloc::string::String>,
  #[prost(string, optional, tag = "2")]
  pub version: ::core::option::Option<::prost::alloc::string::String>,
  #[prost(string, optional, tag = "3")]
  pub release: ::core::option::Option<::prost::alloc::string::String>,
  #[prost(string, optional, tag = "4")]
  pub machine: ::core::option::Option<::prost::alloc::string::String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum BuiltinClock {
  Unknown = 0,
  Realtime = 1,
  RealtimeCoarse = 2,
  Monotonic = 3,
  MonotonicCoarse = 4,
  MonotonicRaw = 5,
  Boottime = 6,
  Tsc = 9,
  Perf = 10,
  MaxId = 63,
}
impl BuiltinClock {
  /// String value of the enum field names used in the ProtoBuf definition.
  ///
  /// The values are not transformed in any way and thus are considered stable
  /// (if the ProtoBuf definition does not change) and safe for programmatic use.
  pub fn as_str_name(&self) -> &'static str {
    match self {
      Self::Unknown => "BUILTIN_CLOCK_UNKNOWN",
      Self::Realtime => "BUILTIN_CLOCK_REALTIME",
      Self::RealtimeCoarse => "BUILTIN_CLOCK_REALTIME_COARSE",
      Self::Monotonic => "BUILTIN_CLOCK_MONOTONIC",
      Self::MonotonicCoarse => "BUILTIN_CLOCK_MONOTONIC_COARSE",
      Self::MonotonicRaw => "BUILTIN_CLOCK_MONOTONIC_RAW",
      Self::Boottime => "BUILTIN_CLOCK_BOOTTIME",
      Self::Tsc => "BUILTIN_CLOCK_TSC",
      Self::Perf => "BUILTIN_CLOCK_PERF",
      Self::MaxId => "BUILTIN_CLOCK_MAX_ID",
    }
  }
  /// Creates an enum from field names used in the ProtoBuf definition.
  pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
    match value {
      "BUILTIN_CLOCK_UNKNOWN" => Some(Self::Unknown),
      "BUILTIN_CLOCK_REALTIME" => Some(Self::Realtime),
      "BUILTIN_CLOCK_REALTIME_COARSE" => Some(Self::RealtimeCoarse),
      "BUILTIN_CLOCK_MONOTONIC" => Some(Self::Monotonic),
      "BUILTIN_CLOCK_MONOTONIC_COARSE" => Some(Self::MonotonicCoarse),
      "BUILTIN_CLOCK_MONOTONIC_RAW" => Some(Self::MonotonicRaw),
      "BUILTIN_CLOCK_BOOTTIME" => Some(Self::Boottime),
      "BUILTIN_CLOCK_TSC" => Some(Self::Tsc),
      "BUILTIN_CLOCK_PERF" => Some(Self::Perf),
      "BUILTIN_CLOCK_MAX_ID" => Some(Self::MaxId),
      _ => None,
    }
  }
}

/// The following fields define the set of enabled trace categories. Each list
/// item is a glob.
///
/// To determine if category is enabled, it is checked against the filters in
/// the following order:
///
///    1. Exact matches in enabled categories.
///    2. Exact matches in enabled tags.
///    3. Exact matches in disabled categories.
///    4. Exact matches in disabled tags.
///    5. Pattern matches in enabled categories.
///    6. Pattern matches in enabled tags.
///    7. Pattern matches in disabled categories.
///    8. Pattern matches in disabled tags.
///
/// If none of the steps produced a match:
///   - In the C++ SDK (`perfetto::Category`), categories are enabled by
///   default.
///   - In the C SDK (`PerfettoTeCategory`), categories are disabled by default.
///
/// Examples:
///
///   - To enable all non-slow/debug categories:
///
///        enabled_categories: "*"
///
///   - To enable specific categories:
///
///        disabled_categories: "*"
///        enabled_categories: "my_category"
///        enabled_categories: "my_category2"
///
///   - To enable only categories with a specific tag:
///
///        disabled_tags: "*"
///        enabled_tags: "my_tag"
///
#[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
pub struct TrackEventConfig {
  /// Default: \[\]
  #[prost(string, repeated, tag = "1")]
  pub disabled_categories: ::prost::alloc::vec::Vec<::prost::alloc::string::String>,
  /// Default: \[\]
  #[prost(string, repeated, tag = "2")]
  pub enabled_categories: ::prost::alloc::vec::Vec<::prost::alloc::string::String>,
  /// Default: \["slow", "debug"\]
  #[prost(string, repeated, tag = "3")]
  pub disabled_tags: ::prost::alloc::vec::Vec<::prost::alloc::string::String>,
  /// Default: \[\]
  #[prost(string, repeated, tag = "4")]
  pub enabled_tags: ::prost::alloc::vec::Vec<::prost::alloc::string::String>,
  /// Default: false (i.e. enabled by default)
  #[prost(bool, optional, tag = "5")]
  pub disable_incremental_timestamps: ::core::option::Option<bool>,
  /// Allows to specify a custom unit different than the default (ns).
  /// Also affects thread timestamps if enable_thread_time_sampling = true.
  /// A multiplier of 1000 means that a timestamp = 3 should be interpreted as
  /// 3000 ns = 3 us.
  /// Default: 1 (if unset, it should be read as 1).
  #[prost(uint64, optional, tag = "6")]
  pub timestamp_unit_multiplier: ::core::option::Option<u64>,
  /// Default: false (i.e. debug_annotations is NOT filtered out by default)
  /// When true, any debug annotations provided as arguments to the
  /// TRACE_EVENT macros are not written into the trace. Typed arguments will
  /// still be emitted even if set to true.
  #[prost(bool, optional, tag = "7")]
  pub filter_debug_annotations: ::core::option::Option<bool>,
  /// Default: false (i.e. disabled)
  /// When true, the SDK samples and emits the current thread time counter value
  /// for each event on the current thread's track. This value represents the
  /// total CPU time consumed by that thread since its creation.
  /// Learn more: "CLOCK_THREAD_CPUTIME_ID" flag at
  /// <https://man7.org/linux/man-pages/man3/clock_gettime.3.html>
  #[prost(bool, optional, tag = "8")]
  pub enable_thread_time_sampling: ::core::option::Option<bool>,
  /// When enable_thread_time_sampling is true, and this is specified, thread
  /// time is sampled only if the elapsed wall time >
  /// `thread_time_subsampling_ns`. Otherwise, thread time is considered nil.
  /// Effectively, this means thread time will have a leeway of
  /// `thread_time_subsampling_ns` and won't be emitted for shorter events.
  #[prost(uint64, optional, tag = "10")]
  pub thread_time_subsampling_ns: ::core::option::Option<u64>,
  /// Default: false (i.e. dynamic event names are NOT filtered out by default)
  /// When true, event_names wrapped in perfetto::DynamicString will be filtered
  /// out.
  #[prost(bool, optional, tag = "9")]
  pub filter_dynamic_event_names: ::core::option::Option<bool>,
}

/// A snapshot of clock readings to allow for trace alignment.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ClockSnapshot {
  #[prost(message, repeated, tag = "1")]
  pub clocks: ::prost::alloc::vec::Vec<clock_snapshot::Clock>,
  /// The authoritative clock domain for the trace. Defaults to BOOTTIME, but can
  /// be overridden in TraceConfig's builtin_data_sources. Trace processor will
  /// attempt to translate packet/event timestamps from various data sources (and
  /// their chosen clock domains) to this domain during import.
  #[prost(enumeration = "BuiltinClock", optional, tag = "2")]
  pub primary_trace_clock: ::core::option::Option<i32>,
}
/// Nested message and enum types in `ClockSnapshot`.
pub mod clock_snapshot {
  #[derive(Clone, Copy, PartialEq, Eq, Hash, ::prost::Message)]
  pub struct Clock {
    /// Clock IDs have the following semantic:
    /// \[1, 63\]:    Builtin types, see BuiltinClock from
    ///              ../common/builtin_clock.proto.
    /// \[64, 127\]:  User-defined clocks. These clocks are sequence-scoped. They
    ///              are only valid within the same |trusted_packet_sequence_id|
    ///              (i.e. only for TracePacket(s) emitted by the same TraceWriter
    ///              that emitted the clock snapshot).
    /// \[128, MAX\]: Reserved for future use. The idea is to allow global clock
    ///              IDs and setting this ID to hash(full_clock_name) & ~127.
    #[prost(uint32, optional, tag = "1")]
    pub clock_id: ::core::option::Option<u32>,
    /// Absolute timestamp. Unit is ns unless specified otherwise by the
    /// unit_multiplier_ns field below.
    #[prost(uint64, optional, tag = "2")]
    pub timestamp: ::core::option::Option<u64>,
    /// When true each TracePacket's timestamp should be interpreted as a delta
    /// from the last TracePacket's timestamp (referencing this clock) emitted by
    /// the same packet_sequence_id. Should only be used for user-defined
    /// sequence-local clocks. The first packet timestamp after each
    /// ClockSnapshot that contains this clock is relative to the |timestamp| in
    /// the ClockSnapshot.
    #[prost(bool, optional, tag = "3")]
    pub is_incremental: ::core::option::Option<bool>,
    /// Allows to specify a custom unit different than the default (ns) for this
    /// clock domain.
    ///
    /// * A multiplier of 1000 means that a timestamp = 3 should be interpreted
    ///    as 3000 ns = 3 us.
    /// * All snapshots for the same clock within a trace need to use the same
    ///    unit.
    /// * `unit_multiplier_ns` is *not* supported for the `primary_trace_clock`.
    #[prost(uint64, optional, tag = "4")]
    pub unit_multiplier_ns: ::core::option::Option<u64>,
  }
  /// Nested message and enum types in `Clock`.
  pub mod clock {
    /// DEPRECATED. This enum has moved to ../common/builtin_clock.proto.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
    #[repr(i32)]
    pub enum BuiltinClocks {
      Unknown = 0,
      Realtime = 1,
      RealtimeCoarse = 2,
      Monotonic = 3,
      MonotonicCoarse = 4,
      MonotonicRaw = 5,
      Boottime = 6,
      BuiltinClockMaxId = 63,
    }
    impl BuiltinClocks {
      /// String value of the enum field names used in the ProtoBuf definition.
      ///
      /// The values are not transformed in any way and thus are considered stable
      /// (if the ProtoBuf definition does not change) and safe for programmatic use.
      pub fn as_str_name(&self) -> &'static str {
        match self {
          Self::Unknown => "UNKNOWN",
          Self::Realtime => "REALTIME",
          Self::RealtimeCoarse => "REALTIME_COARSE",
          Self::Monotonic => "MONOTONIC",
          Self::MonotonicCoarse => "MONOTONIC_COARSE",
          Self::MonotonicRaw => "MONOTONIC_RAW",
          Self::Boottime => "BOOTTIME",
          Self::BuiltinClockMaxId => "BUILTIN_CLOCK_MAX_ID",
        }
      }
      /// Creates an enum from field names used in the ProtoBuf definition.
      pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
        match value {
          "UNKNOWN" => Some(Self::Unknown),
          "REALTIME" => Some(Self::Realtime),
          "REALTIME_COARSE" => Some(Self::RealtimeCoarse),
          "MONOTONIC" => Some(Self::Monotonic),
          "MONOTONIC_COARSE" => Some(Self::MonotonicCoarse),
          "MONOTONIC_RAW" => Some(Self::MonotonicRaw),
          "BOOTTIME" => Some(Self::Boottime),
          "BUILTIN_CLOCK_MAX_ID" => Some(Self::BuiltinClockMaxId),
          _ => None,
        }
      }
    }
  }
}

/// Proto representation of untyped key/value annotations provided in TRACE_EVENT
/// macros. Users of the Perfetto SDK should prefer to use the
/// perfetto::TracedValue API to fill these protos, rather than filling them
/// manually.
///
/// Debug annotations are intended for debug use and are not considered a stable
/// API of the trace contents. Trace-based metrics that use debug annotation
/// values are prone to breakage, so please rely on typed TrackEvent fields for
/// these instead.
///
/// DebugAnnotations support nested arrays and dictionaries. Each entry is
/// encoded as a single DebugAnnotation message. Only dictionary entries
/// set the "name" field. The TrackEvent message forms an implicit root
/// dictionary.
///
/// Example TrackEvent with nested annotations:
///    track_event {
///      debug_annotations {
///        name: "foo"
///        dict_entries {
///          name: "a"
///          bool_value: true
///        }
///        dict_entries {
///          name: "b"
///          int_value: 123
///        }
///      }
///      debug_annotations {
///        name: "bar"
///        array_values {
///          string_value: "hello"
///        }
///        array_values {
///          string_value: "world"
///        }
///      }
///    }
///
/// Next ID: 18.
/// Reserved ID: 15
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct DebugAnnotation {
  #[prost(bytes = "vec", optional, tag = "14")]
  pub proto_value: ::core::option::Option<::prost::alloc::vec::Vec<u8>>,
  #[prost(message, repeated, tag = "11")]
  pub dict_entries: ::prost::alloc::vec::Vec<DebugAnnotation>,
  #[prost(message, repeated, tag = "12")]
  pub array_values: ::prost::alloc::vec::Vec<DebugAnnotation>,
  /// Name fields are set only for dictionary entries.
  #[prost(oneof = "debug_annotation::NameField", tags = "1, 10")]
  pub name_field: ::core::option::Option<debug_annotation::NameField>,
  #[prost(oneof = "debug_annotation::Value", tags = "2, 3, 4, 5, 7, 8, 9, 6, 17")]
  pub value: ::core::option::Option<debug_annotation::Value>,
  /// Used to embed arbitrary proto messages (which are also typically used to
  /// represent typed TrackEvent arguments). |proto_type_name| or
  /// |proto_type_name_iid| are storing the full name of the proto messages (e.g.
  /// .perfetto.protos.DebugAnnotation) and |proto_value| contains the serialised
  /// proto messages. See |TracedValue::WriteProto| for more details.
  #[prost(oneof = "debug_annotation::ProtoTypeDescriptor", tags = "16, 13")]
  pub proto_type_descriptor: ::core::option::Option<debug_annotation::ProtoTypeDescriptor>,
}
/// Nested message and enum types in `DebugAnnotation`.
pub mod debug_annotation {
  /// Deprecated legacy way to use nested values. Only kept for
  /// backwards-compatibility in TraceProcessor. May be removed in the future -
  /// code filling protos should use |dict_entries| and |array_values| instead.
  #[derive(Clone, PartialEq, ::prost::Message)]
  pub struct NestedValue {
    #[prost(enumeration = "nested_value::NestedType", optional, tag = "1")]
    pub nested_type: ::core::option::Option<i32>,
    #[prost(string, repeated, tag = "2")]
    pub dict_keys: ::prost::alloc::vec::Vec<::prost::alloc::string::String>,
    #[prost(message, repeated, tag = "3")]
    pub dict_values: ::prost::alloc::vec::Vec<NestedValue>,
    #[prost(message, repeated, tag = "4")]
    pub array_values: ::prost::alloc::vec::Vec<NestedValue>,
    #[prost(int64, optional, tag = "5")]
    pub int_value: ::core::option::Option<i64>,
    #[prost(double, optional, tag = "6")]
    pub double_value: ::core::option::Option<f64>,
    #[prost(bool, optional, tag = "7")]
    pub bool_value: ::core::option::Option<bool>,
    #[prost(string, optional, tag = "8")]
    pub string_value: ::core::option::Option<::prost::alloc::string::String>,
  }
  /// Nested message and enum types in `NestedValue`.
  pub mod nested_value {
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
    #[repr(i32)]
    pub enum NestedType {
      /// leaf value.
      Unspecified = 0,
      Dict = 1,
      Array = 2,
    }
    impl NestedType {
      /// String value of the enum field names used in the ProtoBuf definition.
      ///
      /// The values are not transformed in any way and thus are considered stable
      /// (if the ProtoBuf definition does not change) and safe for programmatic use.
      pub fn as_str_name(&self) -> &'static str {
        match self {
          Self::Unspecified => "UNSPECIFIED",
          Self::Dict => "DICT",
          Self::Array => "ARRAY",
        }
      }
      /// Creates an enum from field names used in the ProtoBuf definition.
      pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
        match value {
          "UNSPECIFIED" => Some(Self::Unspecified),
          "DICT" => Some(Self::Dict),
          "ARRAY" => Some(Self::Array),
          _ => None,
        }
      }
    }
  }
  /// Name fields are set only for dictionary entries.
  #[derive(Clone, PartialEq, Eq, Hash, ::prost::Oneof)]
  pub enum NameField {
    /// interned DebugAnnotationName.
    #[prost(uint64, tag = "1")]
    NameIid(u64),
    /// non-interned variant.
    #[prost(string, tag = "10")]
    Name(::prost::alloc::string::String),
  }
  #[derive(Clone, PartialEq, ::prost::Oneof)]
  pub enum Value {
    #[prost(bool, tag = "2")]
    BoolValue(bool),
    #[prost(uint64, tag = "3")]
    UintValue(u64),
    #[prost(int64, tag = "4")]
    IntValue(i64),
    #[prost(double, tag = "5")]
    DoubleValue(f64),
    /// Pointers are stored in a separate type as the JSON output treats them
    /// differently from other uint64 values.
    #[prost(uint64, tag = "7")]
    PointerValue(u64),
    /// Deprecated. Use dict_entries / array_values instead.
    #[prost(message, tag = "8")]
    NestedValue(NestedValue),
    /// Legacy instrumentation may not support conversion of nested data to
    /// NestedValue yet.
    #[prost(string, tag = "9")]
    LegacyJsonValue(::prost::alloc::string::String),
    /// interned and non-interned variants of strings.
    #[prost(string, tag = "6")]
    StringValue(::prost::alloc::string::String),
    /// Corresponds to |debug_annotation_string_values| field in InternedData.
    #[prost(uint64, tag = "17")]
    StringValueIid(u64),
  }
  /// Used to embed arbitrary proto messages (which are also typically used to
  /// represent typed TrackEvent arguments). |proto_type_name| or
  /// |proto_type_name_iid| are storing the full name of the proto messages (e.g.
  /// .perfetto.protos.DebugAnnotation) and |proto_value| contains the serialised
  /// proto messages. See |TracedValue::WriteProto| for more details.
  #[derive(Clone, PartialEq, Eq, Hash, ::prost::Oneof)]
  pub enum ProtoTypeDescriptor {
    #[prost(string, tag = "16")]
    ProtoTypeName(::prost::alloc::string::String),
    /// interned DebugAnnotationValueTypeName.
    #[prost(uint64, tag = "13")]
    ProtoTypeNameIid(u64),
  }
}
#[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
pub struct DebugAnnotationName {
  #[prost(uint64, optional, tag = "1")]
  pub iid: ::core::option::Option<u64>,
  #[prost(string, optional, tag = "2")]
  pub name: ::core::option::Option<::prost::alloc::string::String>,
}
/// See the |proto_type_descriptor| comment.
#[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
pub struct DebugAnnotationValueTypeName {
  #[prost(uint64, optional, tag = "1")]
  pub iid: ::core::option::Option<u64>,
  #[prost(string, optional, tag = "2")]
  pub name: ::core::option::Option<::prost::alloc::string::String>,
}
#[derive(Clone, Copy, PartialEq, Eq, Hash, ::prost::Message)]
pub struct LogMessage {
  /// interned SourceLocation.
  #[prost(uint64, optional, tag = "1")]
  pub source_location_iid: ::core::option::Option<u64>,
  /// interned LogMessageBody.
  #[prost(uint64, optional, tag = "2")]
  pub body_iid: ::core::option::Option<u64>,
  #[prost(enumeration = "log_message::Priority", optional, tag = "3")]
  pub prio: ::core::option::Option<i32>,
}
/// Nested message and enum types in `LogMessage`.
pub mod log_message {
  #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
  #[repr(i32)]
  pub enum Priority {
    PrioUnspecified = 0,
    PrioUnused = 1,
    PrioVerbose = 2,
    PrioDebug = 3,
    PrioInfo = 4,
    PrioWarn = 5,
    PrioError = 6,
    PrioFatal = 7,
  }
  impl Priority {
    /// String value of the enum field names used in the ProtoBuf definition.
    ///
    /// The values are not transformed in any way and thus are considered stable
    /// (if the ProtoBuf definition does not change) and safe for programmatic use.
    pub fn as_str_name(&self) -> &'static str {
      match self {
        Self::PrioUnspecified => "PRIO_UNSPECIFIED",
        Self::PrioUnused => "PRIO_UNUSED",
        Self::PrioVerbose => "PRIO_VERBOSE",
        Self::PrioDebug => "PRIO_DEBUG",
        Self::PrioInfo => "PRIO_INFO",
        Self::PrioWarn => "PRIO_WARN",
        Self::PrioError => "PRIO_ERROR",
        Self::PrioFatal => "PRIO_FATAL",
      }
    }
    /// Creates an enum from field names used in the ProtoBuf definition.
    pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
      match value {
        "PRIO_UNSPECIFIED" => Some(Self::PrioUnspecified),
        "PRIO_UNUSED" => Some(Self::PrioUnused),
        "PRIO_VERBOSE" => Some(Self::PrioVerbose),
        "PRIO_DEBUG" => Some(Self::PrioDebug),
        "PRIO_INFO" => Some(Self::PrioInfo),
        "PRIO_WARN" => Some(Self::PrioWarn),
        "PRIO_ERROR" => Some(Self::PrioError),
        "PRIO_FATAL" => Some(Self::PrioFatal),
        _ => None,
      }
    }
  }
}
#[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
pub struct LogMessageBody {
  #[prost(uint64, optional, tag = "1")]
  pub iid: ::core::option::Option<u64>,
  #[prost(string, optional, tag = "2")]
  pub body: ::core::option::Option<::prost::alloc::string::String>,
}
/// TrackEvent arguments describing the execution of a task.
#[derive(Clone, Copy, PartialEq, Eq, Hash, ::prost::Message)]
pub struct TaskExecution {
  /// Source location that the task was posted from.
  /// interned SourceLocation.
  #[prost(uint64, optional, tag = "1")]
  pub posted_from_iid: ::core::option::Option<u64>,
}
/// A source location, represented as a native symbol.
/// This is similar to `message Frame` from
/// protos/perfetto/trace/profiling/profile_common.proto, but for abitrary
/// source code locations (for example in track event args), not stack frames.
#[derive(Clone, Copy, PartialEq, Eq, Hash, ::prost::Message)]
pub struct UnsymbolizedSourceLocation {
  #[prost(uint64, optional, tag = "1")]
  pub iid: ::core::option::Option<u64>,
  #[prost(uint64, optional, tag = "2")]
  pub mapping_id: ::core::option::Option<u64>,
  #[prost(uint64, optional, tag = "3")]
  pub rel_pc: ::core::option::Option<u64>,
}
#[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
pub struct SourceLocation {
  #[prost(uint64, optional, tag = "1")]
  pub iid: ::core::option::Option<u64>,
  #[prost(string, optional, tag = "2")]
  pub file_name: ::core::option::Option<::prost::alloc::string::String>,
  #[prost(string, optional, tag = "3")]
  pub function_name: ::core::option::Option<::prost::alloc::string::String>,
  #[prost(uint32, optional, tag = "4")]
  pub line_number: ::core::option::Option<u32>,
}
/// Trace events emitted by client instrumentation library (TRACE_EVENT macros),
/// which describe activity on a track, such as a thread or asynchronous event
/// track. The track is specified using separate TrackDescriptor messages and
/// referred to via the track's UUID.
///
/// A simple TrackEvent packet specifies a timestamp, category, name and type:
/// ```protobuf
///    trace_packet {
///      timestamp: 1000
///      track_event {
///        categories: \["my_cat"\]
///        name: "my_event"
///        type: TYPE_INSTANT
///       }
///     }
/// ```
///
/// To associate an event with a custom track (e.g. a thread), the track is
/// defined in a separate packet and referred to from the TrackEvent by its UUID:
/// ```protobuf
///    trace_packet {
///      track_descriptor {
///        track_uuid: 1234
///        name: "my_track"
///
///        // Optionally, associate the track with a thread.
///        thread_descriptor {
///          pid: 10
///          tid: 10
///          ..
///        }
///      }
///    }
/// ```
///
/// A pair of TYPE_SLICE_BEGIN and _END events form a slice on the track:
///
/// ```protobuf
///    trace_packet {
///      timestamp: 1200
///      track_event {
///        track_uuid: 1234
///        categories: \["my_cat"\]
///        name: "my_slice"
///        type: TYPE_SLICE_BEGIN
///      }
///    }
///    trace_packet {
///      timestamp: 1400
///      track_event {
///        track_uuid: 1234
///        type: TYPE_SLICE_END
///      }
///    }
/// ```
/// TrackEvents also support optimizations to reduce data repetition and encoded
/// data size, e.g. through data interning (names, categories, ...) and delta
/// encoding of timestamps/counters. For details, see the InternedData message.
/// Further, default values for attributes of events on the same sequence (e.g.
/// their default track association) can be emitted as part of a
/// TrackEventDefaults message.
///
/// Next reserved id: 13 (up to 15). Next id: 57.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct TrackEvent {
  /// Names of categories of the event. In the client library, categories are a
  /// way to turn groups of individual events on or off.
  /// interned EventCategoryName.
  #[prost(uint64, repeated, packed = "false", tag = "3")]
  pub category_iids: ::prost::alloc::vec::Vec<u64>,
  /// non-interned variant.
  #[prost(string, repeated, tag = "22")]
  pub categories: ::prost::alloc::vec::Vec<::prost::alloc::string::String>,
  #[prost(enumeration = "track_event::Type", optional, tag = "9")]
  pub r#type: ::core::option::Option<i32>,
  /// Identifies the track of the event. The default value may be overridden
  /// using TrackEventDefaults, e.g., to specify the track of the TraceWriter's
  /// sequence (in most cases sequence = one thread). If no value is specified
  /// here or in TrackEventDefaults, the TrackEvent will be associated with an
  /// implicit trace-global track (uuid 0). See TrackDescriptor::uuid.
  #[prost(uint64, optional, tag = "11")]
  pub track_uuid: ::core::option::Option<u64>,
  /// To encode counter values more efficiently, we support attaching additional
  /// counter values to a TrackEvent of any type. All values will share the same
  /// timestamp specified in the TracePacket. The value at
  /// extra_counter_values\[N\] is for the counter track referenced by
  /// extra_counter_track_uuids\[N\].
  ///
  /// |extra_counter_track_uuids| may also be set via TrackEventDefaults. There
  /// should always be equal or more uuids than values. It is valid to set more
  /// uuids (e.g. via defaults) than values. If uuids are specified in
  /// TrackEventDefaults and a TrackEvent, the TrackEvent uuids override the
  /// default uuid list.
  ///
  /// For example, this allows snapshotting the thread time clock at each
  /// thread-track BEGIN and END event to capture the cpu time delta of a slice.
  #[prost(uint64, repeated, packed = "false", tag = "31")]
  pub extra_counter_track_uuids: ::prost::alloc::vec::Vec<u64>,
  #[prost(int64, repeated, packed = "false", tag = "12")]
  pub extra_counter_values: ::prost::alloc::vec::Vec<i64>,
  /// Counter snapshots using floating point instead of integer values.
  #[prost(uint64, repeated, packed = "false", tag = "45")]
  pub extra_double_counter_track_uuids: ::prost::alloc::vec::Vec<u64>,
  #[prost(double, repeated, packed = "false", tag = "46")]
  pub extra_double_counter_values: ::prost::alloc::vec::Vec<f64>,
  /// IDs of flows originating, passing through, or ending at this event.
  /// Flow IDs are global within a trace.
  ///
  /// A flow connects a sequence of TrackEvents within or across tracks, e.g.
  /// an input event may be handled on one thread but cause another event on
  /// a different thread - a flow between the two events can associate them.
  ///
  /// The direction of the flows between events is inferred from the events'
  /// timestamps. The earliest event with the same flow ID becomes the source
  /// of the flow. Any events thereafter are intermediate steps of the flow,
  /// until the flow terminates at the last event with the flow ID.
  ///
  /// Flows can also be explicitly terminated (see |terminating_flow_ids|), so
  /// that the same ID can later be reused for another flow.
  /// DEPRECATED. Only kept for backwards compatibility. Use |flow_ids|.
  #[deprecated]
  #[prost(uint64, repeated, packed = "false", tag = "36")]
  pub flow_ids_old: ::prost::alloc::vec::Vec<u64>,
  /// TODO(b/204341740): replace "flow_ids_old" with "flow_ids" to reduce memory
  /// consumption.
  #[prost(fixed64, repeated, packed = "false", tag = "47")]
  pub flow_ids: ::prost::alloc::vec::Vec<u64>,
  /// List of flow ids which should terminate on this event, otherwise same as
  /// |flow_ids|.
  /// Any one flow ID should be either listed as part of |flow_ids| OR
  /// |terminating_flow_ids|, not both.
  /// DEPRECATED. Only kept for backwards compatibility.  Use
  /// |terminating_flow_ids|.
  #[deprecated]
  #[prost(uint64, repeated, packed = "false", tag = "42")]
  pub terminating_flow_ids_old: ::prost::alloc::vec::Vec<u64>,
  /// TODO(b/204341740): replace "terminating_flow_ids_old" with
  /// "terminating_flow_ids" to reduce memory consumption.
  #[prost(fixed64, repeated, packed = "false", tag = "48")]
  pub terminating_flow_ids: ::prost::alloc::vec::Vec<u64>,
  /// Debug annotations associated with this event. These are arbitrary key-value
  /// pairs that can be used to attach additional information to the event.
  /// See DebugAnnotation message for details on supported value types.
  ///
  /// For example, debug annotations can be used to attach a URL or resource
  /// identifier to a network request event. Arrays, dictionaries and full
  /// nested structures (e.g. arrays of dictionaries of dictionaries)
  /// are supported.
  #[prost(message, repeated, tag = "4")]
  pub debug_annotations: ::prost::alloc::vec::Vec<DebugAnnotation>,
  /// Typed event arguments:
  #[prost(message, optional, tag = "5")]
  pub task_execution: ::core::option::Option<TaskExecution>,
  #[prost(message, optional, tag = "21")]
  pub log_message: ::core::option::Option<LogMessage>,
  #[prost(message, optional, tag = "6")]
  pub legacy_event: ::core::option::Option<track_event::LegacyEvent>,
  /// Optional name of the event for its display in trace viewer. May be left
  /// unspecified for events with typed arguments.
  ///
  /// Note that metrics should not rely on event names, as they are prone to
  /// changing. Instead, they should use typed arguments to identify the events
  /// they are interested in.
  #[prost(oneof = "track_event::NameField", tags = "10, 23")]
  pub name_field: ::core::option::Option<track_event::NameField>,
  /// A new value for a counter track. |track_uuid| should refer to a track with
  /// a CounterDescriptor, and |type| should be TYPE_COUNTER. For a more
  /// efficient encoding of counter values that are sampled at the beginning/end
  /// of a slice, see |extra_counter_values| and |extra_counter_track_uuids|.
  /// Counter values can optionally be encoded in as delta values (positive or
  /// negative) on each packet sequence (see CounterIncrementalBase).
  #[prost(oneof = "track_event::CounterValueField", tags = "30, 44")]
  pub counter_value_field: ::core::option::Option<track_event::CounterValueField>,
  /// An opaque identifier to correlate this slice with other slices that are
  /// considered part of the same logical operation, even if they are not
  /// causally connected. Examples uses of a correlation id might be the number
  /// of frame going through various stages of rendering in a GPU, the id for an
  /// RPC request going through a distributed system, or the id of a network
  /// request going through various stages of processing by the kernel.
  ///
  /// NOTE: if the events *are* causually connected, you probably want to use
  /// flows instead of OR in addition to correlation ids.
  ///
  /// UIs can use this identifier to visually link these slices, for instance,
  /// by assigning them a consistent color or highlighting the entire correlated
  /// set when one slice is hovered.
  ///
  /// Only one field within this 'oneof' should be set to define the correlation.
  #[prost(oneof = "track_event::CorrelationIdField", tags = "52, 53, 54")]
  pub correlation_id_field: ::core::option::Option<track_event::CorrelationIdField>,
  /// Callstack associated with this event. This captures the program stack at
  /// the time the event occurred, useful for understanding what code path led
  /// to the event.
  ///
  /// Two variants are supported:
  /// - callstack: Inline callstack data (simpler when trace size is not a
  ///    concern or callstacks are unique)
  /// - callstack_iid: Reference to an interned Callstack in InternedData
  ///    (efficient for repeated callstacks)
  ///
  /// Only one of these fields should be set.
  #[prost(oneof = "track_event::CallstackField", tags = "55, 56")]
  pub callstack_field: ::core::option::Option<track_event::CallstackField>,
  /// This field is used only if the source location represents the function that
  /// executes during this event.
  #[prost(oneof = "track_event::SourceLocationField", tags = "33, 34")]
  pub source_location_field: ::core::option::Option<track_event::SourceLocationField>,
  /// Deprecated. Use the |timestamp| and |timestamp_clock_id| fields in
  /// TracePacket instead.
  ///
  /// Timestamp in microseconds (usually CLOCK_MONOTONIC).
  #[prost(oneof = "track_event::Timestamp", tags = "1, 16")]
  pub timestamp: ::core::option::Option<track_event::Timestamp>,
  /// Deprecated. Use |extra_counter_values| and |extra_counter_track_uuids| to
  /// encode thread time instead.
  ///
  /// CPU time for the current thread (e.g., CLOCK_THREAD_CPUTIME_ID) in
  /// microseconds.
  #[prost(oneof = "track_event::ThreadTime", tags = "2, 17")]
  pub thread_time: ::core::option::Option<track_event::ThreadTime>,
  /// Deprecated. Use |extra_counter_values| and |extra_counter_track_uuids| to
  /// encode thread instruction count instead.
  ///
  /// Value of the instruction counter for the current thread.
  #[prost(oneof = "track_event::ThreadInstructionCount", tags = "8, 20")]
  pub thread_instruction_count: ::core::option::Option<track_event::ThreadInstructionCount>,
}
/// Nested message and enum types in `TrackEvent`.
pub mod track_event {
  /// Inline callstack for TrackEvents when interning is not needed.
  /// This is a simplified version of the profiling Callstack/Frame messages,
  /// designed for cases where trace size is not critical or callstacks are
  /// unique.
  ///
  /// Use this for simple callstacks with function names and source locations.
  /// For binary/library information (mappings, build IDs, relative PCs), use
  /// interned callstacks via callstack_iid instead.
  #[derive(Clone, PartialEq, ::prost::Message)]
  pub struct Callstack {
    /// Frames of this callstack, ordered from bottom (outermost) to top
    /// (innermost). For example, if main() calls foo() which calls bar(), the
    /// frames would be: \[main, foo, bar\]
    #[prost(message, repeated, tag = "1")]
    pub frames: ::prost::alloc::vec::Vec<callstack::Frame>,
  }
  /// Nested message and enum types in `Callstack`.
  pub mod callstack {
    /// Frame within an inline callstack.
    #[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
    pub struct Frame {
      /// Function name, e.g., "malloc" or "std::vector<int>::push_back"
      #[prost(string, optional, tag = "1")]
      pub function_name: ::core::option::Option<::prost::alloc::string::String>,
      /// Optional: Source file path, e.g., "/src/foo.cc"
      #[prost(string, optional, tag = "2")]
      pub source_file: ::core::option::Option<::prost::alloc::string::String>,
      /// Optional: Line number in the source file
      #[prost(uint32, optional, tag = "3")]
      pub line_number: ::core::option::Option<u32>,
    }
  }
  /// Apart from {category, time, thread time, tid, pid}, other legacy trace
  /// event attributes are initially simply proxied for conversion to a JSON
  /// trace. We intend to gradually transition these attributes to similar native
  /// features in TrackEvent (e.g. async + flow events), or deprecate them
  /// without replacement where transition is unsuitable.
  ///
  /// Next reserved id: 16 (up to 16).
  /// Next id: 20.
  #[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
  pub struct LegacyEvent {
    /// Deprecated, use TrackEvent::name(_iid) instead.
    /// interned EventName.
    #[prost(uint64, optional, tag = "1")]
    pub name_iid: ::core::option::Option<u64>,
    #[prost(int32, optional, tag = "2")]
    pub phase: ::core::option::Option<i32>,
    #[prost(int64, optional, tag = "3")]
    pub duration_us: ::core::option::Option<i64>,
    #[prost(int64, optional, tag = "4")]
    pub thread_duration_us: ::core::option::Option<i64>,
    /// Elapsed retired instruction count during the event.
    #[prost(int64, optional, tag = "15")]
    pub thread_instruction_delta: ::core::option::Option<i64>,
    /// Additional optional scope for |id|.
    #[prost(string, optional, tag = "7")]
    pub id_scope: ::core::option::Option<::prost::alloc::string::String>,
    /// Consider the thread timestamps for async BEGIN/END event pairs as valid.
    #[prost(bool, optional, tag = "9")]
    pub use_async_tts: ::core::option::Option<bool>,
    /// Idenfifies a flow. Flow events with the same bind_id are connected.
    #[prost(uint64, optional, tag = "8")]
    pub bind_id: ::core::option::Option<u64>,
    /// Use the enclosing slice as binding point for a flow end event instead of
    /// the next slice. Flow start/step events always bind to the enclosing
    /// slice.
    #[prost(bool, optional, tag = "12")]
    pub bind_to_enclosing: ::core::option::Option<bool>,
    #[prost(enumeration = "legacy_event::FlowDirection", optional, tag = "13")]
    pub flow_direction: ::core::option::Option<i32>,
    #[prost(enumeration = "legacy_event::InstantEventScope", optional, tag = "14")]
    pub instant_event_scope: ::core::option::Option<i32>,
    /// Override the pid/tid if the writer needs to emit events on behalf of
    /// another process/thread. This should be the exception. Normally, the
    /// pid+tid from ThreadDescriptor is used.
    #[prost(int32, optional, tag = "18")]
    pub pid_override: ::core::option::Option<i32>,
    #[prost(int32, optional, tag = "19")]
    pub tid_override: ::core::option::Option<i32>,
    #[prost(oneof = "legacy_event::Id", tags = "6, 10, 11")]
    pub id: ::core::option::Option<legacy_event::Id>,
  }
  /// Nested message and enum types in `LegacyEvent`.
  pub mod legacy_event {
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
    #[repr(i32)]
    pub enum FlowDirection {
      FlowUnspecified = 0,
      FlowIn = 1,
      FlowOut = 2,
      FlowInout = 3,
    }
    impl FlowDirection {
      /// String value of the enum field names used in the ProtoBuf definition.
      ///
      /// The values are not transformed in any way and thus are considered stable
      /// (if the ProtoBuf definition does not change) and safe for programmatic use.
      pub fn as_str_name(&self) -> &'static str {
        match self {
          Self::FlowUnspecified => "FLOW_UNSPECIFIED",
          Self::FlowIn => "FLOW_IN",
          Self::FlowOut => "FLOW_OUT",
          Self::FlowInout => "FLOW_INOUT",
        }
      }
      /// Creates an enum from field names used in the ProtoBuf definition.
      pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
        match value {
          "FLOW_UNSPECIFIED" => Some(Self::FlowUnspecified),
          "FLOW_IN" => Some(Self::FlowIn),
          "FLOW_OUT" => Some(Self::FlowOut),
          "FLOW_INOUT" => Some(Self::FlowInout),
          _ => None,
        }
      }
    }
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
    #[repr(i32)]
    pub enum InstantEventScope {
      ScopeUnspecified = 0,
      ScopeGlobal = 1,
      ScopeProcess = 2,
      ScopeThread = 3,
    }
    impl InstantEventScope {
      /// String value of the enum field names used in the ProtoBuf definition.
      ///
      /// The values are not transformed in any way and thus are considered stable
      /// (if the ProtoBuf definition does not change) and safe for programmatic use.
      pub fn as_str_name(&self) -> &'static str {
        match self {
          Self::ScopeUnspecified => "SCOPE_UNSPECIFIED",
          Self::ScopeGlobal => "SCOPE_GLOBAL",
          Self::ScopeProcess => "SCOPE_PROCESS",
          Self::ScopeThread => "SCOPE_THREAD",
        }
      }
      /// Creates an enum from field names used in the ProtoBuf definition.
      pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
        match value {
          "SCOPE_UNSPECIFIED" => Some(Self::ScopeUnspecified),
          "SCOPE_GLOBAL" => Some(Self::ScopeGlobal),
          "SCOPE_PROCESS" => Some(Self::ScopeProcess),
          "SCOPE_THREAD" => Some(Self::ScopeThread),
          _ => None,
        }
      }
    }
    #[derive(Clone, Copy, PartialEq, Eq, Hash, ::prost::Oneof)]
    pub enum Id {
      #[prost(uint64, tag = "6")]
      UnscopedId(u64),
      #[prost(uint64, tag = "10")]
      LocalId(u64),
      #[prost(uint64, tag = "11")]
      GlobalId(u64),
    }
  }
  /// Type of the TrackEvent (required if |phase| in LegacyEvent is not set).
  #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
  #[repr(i32)]
  pub enum Type {
    Unspecified = 0,
    /// Slice events are events that have a begin and end timestamp, i.e. a
    /// duration. They can be nested similar to a callstack: If, on the same
    /// track, event B begins after event A, but before A ends, B is a child
    /// event of A and will be drawn as a nested event underneath A in the UI.
    /// Note that child events should always end before their parents (e.g. B
    /// before A).
    ///
    /// Each slice event is formed by a pair of BEGIN + END events. The END event
    /// does not need to repeat any TrackEvent fields it has in common with its
    /// corresponding BEGIN event. Arguments and debug annotations of the BEGIN +
    /// END pair will be merged during trace import.
    ///
    /// Note that we deliberately chose not to support COMPLETE events (which
    /// would specify a duration directly) since clients would need to delay
    /// writing them until the slice is completed, which can result in reordered
    /// events in the trace and loss of unfinished events at the end of a trace.
    SliceBegin = 1,
    SliceEnd = 2,
    /// Instant events are nestable events without duration. They can be children
    /// of slice events on the same track.
    Instant = 3,
    /// Event that provides a value for a counter track. |track_uuid| should
    /// refer to a counter track and |counter_value| set to the new value. Note
    /// that most other TrackEvent fields (e.g. categories, name, ..) are not
    /// supported for TYPE_COUNTER events. See also CounterDescriptor.
    Counter = 4,
  }
  impl Type {
    /// String value of the enum field names used in the ProtoBuf definition.
    ///
    /// The values are not transformed in any way and thus are considered stable
    /// (if the ProtoBuf definition does not change) and safe for programmatic use.
    pub fn as_str_name(&self) -> &'static str {
      match self {
        Self::Unspecified => "TYPE_UNSPECIFIED",
        Self::SliceBegin => "TYPE_SLICE_BEGIN",
        Self::SliceEnd => "TYPE_SLICE_END",
        Self::Instant => "TYPE_INSTANT",
        Self::Counter => "TYPE_COUNTER",
      }
    }
    /// Creates an enum from field names used in the ProtoBuf definition.
    pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
      match value {
        "TYPE_UNSPECIFIED" => Some(Self::Unspecified),
        "TYPE_SLICE_BEGIN" => Some(Self::SliceBegin),
        "TYPE_SLICE_END" => Some(Self::SliceEnd),
        "TYPE_INSTANT" => Some(Self::Instant),
        "TYPE_COUNTER" => Some(Self::Counter),
        _ => None,
      }
    }
  }
  /// Optional name of the event for its display in trace viewer. May be left
  /// unspecified for events with typed arguments.
  ///
  /// Note that metrics should not rely on event names, as they are prone to
  /// changing. Instead, they should use typed arguments to identify the events
  /// they are interested in.
  #[derive(Clone, PartialEq, Eq, Hash, ::prost::Oneof)]
  pub enum NameField {
    /// interned EventName.
    #[prost(uint64, tag = "10")]
    NameIid(u64),
    /// non-interned variant.
    #[prost(string, tag = "23")]
    Name(::prost::alloc::string::String),
  }
  /// A new value for a counter track. |track_uuid| should refer to a track with
  /// a CounterDescriptor, and |type| should be TYPE_COUNTER. For a more
  /// efficient encoding of counter values that are sampled at the beginning/end
  /// of a slice, see |extra_counter_values| and |extra_counter_track_uuids|.
  /// Counter values can optionally be encoded in as delta values (positive or
  /// negative) on each packet sequence (see CounterIncrementalBase).
  #[derive(Clone, Copy, PartialEq, ::prost::Oneof)]
  pub enum CounterValueField {
    #[prost(int64, tag = "30")]
    CounterValue(i64),
    #[prost(double, tag = "44")]
    DoubleCounterValue(f64),
  }
  /// An opaque identifier to correlate this slice with other slices that are
  /// considered part of the same logical operation, even if they are not
  /// causally connected. Examples uses of a correlation id might be the number
  /// of frame going through various stages of rendering in a GPU, the id for an
  /// RPC request going through a distributed system, or the id of a network
  /// request going through various stages of processing by the kernel.
  ///
  /// NOTE: if the events *are* causually connected, you probably want to use
  /// flows instead of OR in addition to correlation ids.
  ///
  /// UIs can use this identifier to visually link these slices, for instance,
  /// by assigning them a consistent color or highlighting the entire correlated
  /// set when one slice is hovered.
  ///
  /// Only one field within this 'oneof' should be set to define the correlation.
  #[derive(Clone, PartialEq, Eq, Hash, ::prost::Oneof)]
  pub enum CorrelationIdField {
    /// A 64-bit unsigned integer used as the correlation ID.
    ///
    /// Best for performance and compact traces if the identifier is naturally
    /// numerical or can be easily mapped to one by the trace producer.
    #[prost(uint64, tag = "52")]
    CorrelationId(u64),
    /// A string value used as the correlation ID.
    ///
    /// Offers maximum flexibility for human-readable or complex identifiers
    /// (e.g., GUIDs). Note: Using many unique, long strings may increase trace
    /// size. For frequently repeated string identifiers, consider
    /// 'correlation_id_string_iid'.
    #[prost(string, tag = "53")]
    CorrelationIdStr(::prost::alloc::string::String),
    /// An interned string identifier (an IID) for correlation.
    ///
    /// This 64-bit ID refers to a string defined in the 'correlation_id_str'
    /// field within the packet sequence's InternedData. This approach combines
    /// the descriptiveness and uniqueness of strings with the efficiency of
    /// integer IDs for storage and comparison, especially for identifiers that
    /// repeat across many events.
    #[prost(uint64, tag = "54")]
    CorrelationIdStrIid(u64),
  }
  /// Callstack associated with this event. This captures the program stack at
  /// the time the event occurred, useful for understanding what code path led
  /// to the event.
  ///
  /// Two variants are supported:
  /// - callstack: Inline callstack data (simpler when trace size is not a
  ///    concern or callstacks are unique)
  /// - callstack_iid: Reference to an interned Callstack in InternedData
  ///    (efficient for repeated callstacks)
  ///
  /// Only one of these fields should be set.
  #[derive(Clone, PartialEq, ::prost::Oneof)]
  pub enum CallstackField {
    /// Inline callstack data. Use this for simplicity when interning is not
    /// needed (e.g., for unique callstacks or when trace size is not critical).
    #[prost(message, tag = "55")]
    Callstack(Callstack),
    /// Reference to interned Callstack (see InternedData.callstacks).
    /// This is the efficient option when callstacks are repeated.
    ///
    /// Note: iids *always* start from 1. A value of 0 is considered "not set".
    #[prost(uint64, tag = "56")]
    CallstackIid(u64),
  }
  /// This field is used only if the source location represents the function that
  /// executes during this event.
  #[derive(Clone, PartialEq, Eq, Hash, ::prost::Oneof)]
  pub enum SourceLocationField {
    /// Non-interned field.
    #[prost(message, tag = "33")]
    SourceLocation(super::SourceLocation),
    /// Interned field.
    #[prost(uint64, tag = "34")]
    SourceLocationIid(u64),
  }
  /// Deprecated. Use the |timestamp| and |timestamp_clock_id| fields in
  /// TracePacket instead.
  ///
  /// Timestamp in microseconds (usually CLOCK_MONOTONIC).
  #[derive(Clone, Copy, PartialEq, Eq, Hash, ::prost::Oneof)]
  pub enum Timestamp {
    /// Delta timestamp value since the last TrackEvent or ThreadDescriptor. To
    /// calculate the absolute timestamp value, sum up all delta values of the
    /// preceding TrackEvents since the last ThreadDescriptor and add the sum to
    /// the |reference_timestamp| in ThreadDescriptor. This value should always
    /// be positive.
    #[prost(int64, tag = "1")]
    TimestampDeltaUs(i64),
    /// Absolute value (e.g. a manually specified timestamp in the macro).
    /// This is a one-off value that does not affect delta timestamp computation
    /// in subsequent TrackEvents.
    #[prost(int64, tag = "16")]
    TimestampAbsoluteUs(i64),
  }
  /// Deprecated. Use |extra_counter_values| and |extra_counter_track_uuids| to
  /// encode thread time instead.
  ///
  /// CPU time for the current thread (e.g., CLOCK_THREAD_CPUTIME_ID) in
  /// microseconds.
  #[derive(Clone, Copy, PartialEq, Eq, Hash, ::prost::Oneof)]
  pub enum ThreadTime {
    /// Delta timestamp value since the last TrackEvent or ThreadDescriptor. To
    /// calculate the absolute timestamp value, sum up all delta values of the
    /// preceding TrackEvents since the last ThreadDescriptor and add the sum to
    /// the |reference_timestamp| in ThreadDescriptor. This value should always
    /// be positive.
    #[prost(int64, tag = "2")]
    ThreadTimeDeltaUs(i64),
    /// This is a one-off absolute value that does not affect delta timestamp
    /// computation in subsequent TrackEvents.
    #[prost(int64, tag = "17")]
    ThreadTimeAbsoluteUs(i64),
  }
  /// Deprecated. Use |extra_counter_values| and |extra_counter_track_uuids| to
  /// encode thread instruction count instead.
  ///
  /// Value of the instruction counter for the current thread.
  #[derive(Clone, Copy, PartialEq, Eq, Hash, ::prost::Oneof)]
  pub enum ThreadInstructionCount {
    /// Same encoding as |thread_time| field above.
    #[prost(int64, tag = "8")]
    ThreadInstructionCountDelta(i64),
    #[prost(int64, tag = "20")]
    ThreadInstructionCountAbsolute(i64),
  }
}
/// Default values for fields of all TrackEvents on the same packet sequence.
/// Should be emitted as part of TracePacketDefaults whenever incremental state
/// is cleared. It's defined here because field IDs should match those of the
/// corresponding fields in TrackEvent.
#[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
pub struct TrackEventDefaults {
  #[prost(uint64, optional, tag = "11")]
  pub track_uuid: ::core::option::Option<u64>,
  #[prost(uint64, repeated, packed = "false", tag = "31")]
  pub extra_counter_track_uuids: ::prost::alloc::vec::Vec<u64>,
  #[prost(uint64, repeated, packed = "false", tag = "45")]
  pub extra_double_counter_track_uuids: ::prost::alloc::vec::Vec<u64>,
}
#[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
pub struct EventCategory {
  #[prost(uint64, optional, tag = "1")]
  pub iid: ::core::option::Option<u64>,
  #[prost(string, optional, tag = "2")]
  pub name: ::core::option::Option<::prost::alloc::string::String>,
}
#[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
pub struct EventName {
  #[prost(uint64, optional, tag = "1")]
  pub iid: ::core::option::Option<u64>,
  #[prost(string, optional, tag = "2")]
  pub name: ::core::option::Option<::prost::alloc::string::String>,
}
/// The interning fields in this file can refer to 2 different intern tables,
/// depending on the message they are used in. If the interned fields are present
/// in ProfilePacket proto, then the intern tables included in the ProfilePacket
/// should be used. If the intered fields are present in the
/// StreamingProfilePacket proto, then the intern tables included in all of the
/// previous InternedData message with same sequence ID should be used.
/// TODO(fmayer): Move to the intern tables to a common location.
#[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
pub struct InternedString {
  /// Interning key. Starts from 1, 0 is the same as "not set".
  #[prost(uint64, optional, tag = "1")]
  pub iid: ::core::option::Option<u64>,
  /// The actual string.
  #[prost(bytes = "vec", optional, tag = "2")]
  pub str: ::core::option::Option<::prost::alloc::vec::Vec<u8>>,
}
/// Source line info.
#[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
pub struct Line {
  #[prost(string, optional, tag = "1")]
  pub function_name: ::core::option::Option<::prost::alloc::string::String>,
  #[prost(string, optional, tag = "2")]
  pub source_file_name: ::core::option::Option<::prost::alloc::string::String>,
  #[prost(uint32, optional, tag = "3")]
  pub line_number: ::core::option::Option<u32>,
}
/// Symbols for a given address in a module.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct AddressSymbols {
  #[prost(uint64, optional, tag = "1")]
  pub address: ::core::option::Option<u64>,
  /// Source lines that correspond to this address.
  ///
  /// These are repeated because when inlining happens, multiple functions'
  /// frames can be at a single address. Imagine function Foo calling the
  /// std::vector<int> constructor, which gets inlined at 0xf00. We then get
  /// both Foo and the std::vector<int> constructor when we symbolize the
  /// address.
  #[prost(message, repeated, tag = "2")]
  pub lines: ::prost::alloc::vec::Vec<Line>,
}
/// Symbols for addresses seen in a module.
/// Used in re-symbolisation of complete traces.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ModuleSymbols {
  /// Fully qualified path to the mapping.
  /// E.g. /system/lib64/libc.so.
  #[prost(string, optional, tag = "1")]
  pub path: ::core::option::Option<::prost::alloc::string::String>,
  /// .note.gnu.build-id on Linux (not hex encoded).
  /// uuid on MacOS.
  /// Module GUID on Windows.
  #[prost(string, optional, tag = "2")]
  pub build_id: ::core::option::Option<::prost::alloc::string::String>,
  #[prost(message, repeated, tag = "3")]
  pub address_symbols: ::prost::alloc::vec::Vec<AddressSymbols>,
}
#[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
pub struct Mapping {
  /// Interning key.
  /// Starts from 1, 0 is the same as "not set".
  #[prost(uint64, optional, tag = "1")]
  pub iid: ::core::option::Option<u64>,
  /// Interning key.
  /// Starts from 1, 0 is the same as "not set".
  #[prost(uint64, optional, tag = "2")]
  pub build_id: ::core::option::Option<u64>,
  /// This is not set on Android 10.
  #[prost(uint64, optional, tag = "8")]
  pub exact_offset: ::core::option::Option<u64>,
  #[prost(uint64, optional, tag = "3")]
  pub start_offset: ::core::option::Option<u64>,
  #[prost(uint64, optional, tag = "4")]
  pub start: ::core::option::Option<u64>,
  #[prost(uint64, optional, tag = "5")]
  pub end: ::core::option::Option<u64>,
  /// Libunwindstack-specific concept, not to be confused with bionic linker's
  /// notion of load_bias. Needed to correct relative pc addresses (as produced
  /// by libunwindstack) when doing offline resymbolisation.
  ///
  /// For an executable ELF PT_LOAD segment, this is:
  ///    p_vaddr - p_offset
  ///
  /// Where p_offset means that the code is at that offset into the ELF file on
  /// disk. While p_vaddr is the offset at which the code gets *mapped*, relative
  /// to where the linker loads the ELF into the address space. For most ELFs,
  /// the two values are identical and therefore load_bias is zero.
  #[prost(uint64, optional, tag = "6")]
  pub load_bias: ::core::option::Option<u64>,
  /// E.g. \["system", "lib64", "libc.so"\]
  /// id of string.
  #[prost(uint64, repeated, packed = "false", tag = "7")]
  pub path_string_ids: ::prost::alloc::vec::Vec<u64>,
}
#[derive(Clone, Copy, PartialEq, Eq, Hash, ::prost::Message)]
pub struct Frame {
  /// Interning key. Starts from 1, 0 is the same as "not set".
  #[prost(uint64, optional, tag = "1")]
  pub iid: ::core::option::Option<u64>,
  /// E.g. "fopen"
  /// id of string.
  #[prost(uint64, optional, tag = "2")]
  pub function_name_id: ::core::option::Option<u64>,
  /// The mapping in which this frame's instruction pointer resides.
  /// iid of Mapping.iid.
  ///
  /// If set (non-zero), rel_pc MUST also be set. If mapping_id is 0 (not set),
  /// this frame has no associated memory mapping (e.g., symbolized frames
  /// without address information).
  ///
  /// Starts from 1, 0 is the same as "not set".
  #[prost(uint64, optional, tag = "3")]
  pub mapping_id: ::core::option::Option<u64>,
  /// Instruction pointer relative to the start of the mapping.
  /// MUST be set if mapping_id is set (non-zero). Ignored if mapping_id is 0.
  #[prost(uint64, optional, tag = "4")]
  pub rel_pc: ::core::option::Option<u64>,
  /// Source file path for this frame.
  /// This is typically set during online symbolization when symbol information
  /// is available at trace collection time. If not set, source file paths may be
  /// added later via offline symbolization (see ModuleSymbols).
  ///
  /// Starts from 1, 0 is the same as "not set".
  ///
  /// iid of InternedData.source_paths.
  #[prost(uint64, optional, tag = "5")]
  pub source_path_iid: ::core::option::Option<u64>,
  /// Line number in the source file for this frame.
  /// This is typically set during online symbolization when symbol information
  /// is available at trace collection time. If not set, line numbers may be
  /// added later via offline symbolization (see ModuleSymbols).
  #[prost(uint32, optional, tag = "6")]
  pub line_number: ::core::option::Option<u32>,
}
#[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
pub struct Callstack {
  /// Interning key. Starts from 1, 0 is the same as "not set".
  #[prost(uint64, optional, tag = "1")]
  pub iid: ::core::option::Option<u64>,
  /// Frames of this callstack. Bottom frame first.
  #[prost(uint64, repeated, packed = "false", tag = "2")]
  pub frame_ids: ::prost::alloc::vec::Vec<u64>,
}
/// Message that contains new entries for the interning indices of a packet
/// sequence.
///
/// The writer will usually emit new entries in the same TracePacket that first
/// refers to them (since the last reset of interning state). They may also be
/// emitted proactively in advance of referring to them in later packets.
///
/// Next reserved id: 8 (up to 15).
/// Next id: 44.
///
/// TODO(eseckler): Replace iid fields inside interned messages with
/// map<iid, message> type fields in InternedData.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct InternedData {
  /// Each field's message type needs to specify an |iid| field, which is the ID
  /// of the entry in the field's interning index. Each field constructs its own
  /// index, thus interning IDs are scoped to the tracing session and field
  /// (usually as a counter for efficient var-int encoding), and optionally to
  /// the incremental state generation of the packet sequence.
  #[prost(message, repeated, tag = "1")]
  pub event_categories: ::prost::alloc::vec::Vec<EventCategory>,
  #[prost(message, repeated, tag = "2")]
  pub event_names: ::prost::alloc::vec::Vec<EventName>,
  #[prost(message, repeated, tag = "3")]
  pub debug_annotation_names: ::prost::alloc::vec::Vec<DebugAnnotationName>,
  #[prost(message, repeated, tag = "27")]
  pub debug_annotation_value_type_names: ::prost::alloc::vec::Vec<DebugAnnotationValueTypeName>,
  #[prost(message, repeated, tag = "4")]
  pub source_locations: ::prost::alloc::vec::Vec<SourceLocation>,
  #[prost(message, repeated, tag = "28")]
  pub unsymbolized_source_locations: ::prost::alloc::vec::Vec<UnsymbolizedSourceLocation>,
  #[prost(message, repeated, tag = "20")]
  pub log_message_body: ::prost::alloc::vec::Vec<LogMessageBody>,
  /// Build IDs of exectuable files.
  #[prost(message, repeated, tag = "16")]
  pub build_ids: ::prost::alloc::vec::Vec<InternedString>,
  /// Paths to executable files.
  #[prost(message, repeated, tag = "17")]
  pub mapping_paths: ::prost::alloc::vec::Vec<InternedString>,
  /// Paths to source files.
  #[prost(message, repeated, tag = "18")]
  pub source_paths: ::prost::alloc::vec::Vec<InternedString>,
  /// Names of functions used in frames below.
  #[prost(message, repeated, tag = "5")]
  pub function_names: ::prost::alloc::vec::Vec<InternedString>,
  /// Executable files mapped into processes.
  #[prost(message, repeated, tag = "19")]
  pub mappings: ::prost::alloc::vec::Vec<Mapping>,
  /// Frames of callstacks of a program.
  #[prost(message, repeated, tag = "6")]
  pub frames: ::prost::alloc::vec::Vec<Frame>,
  /// A callstack of a program.
  #[prost(message, repeated, tag = "7")]
  pub callstacks: ::prost::alloc::vec::Vec<Callstack>,
  /// Additional Vulkan information sent in a VulkanMemoryEvent message
  #[prost(message, repeated, tag = "22")]
  pub vulkan_memory_keys: ::prost::alloc::vec::Vec<InternedString>,
  /// This is set when FtraceConfig.symbolize_ksyms = true.
  /// The id of each symbol the number that will be reported in ftrace events
  /// like sched_block_reason.caller and is obtained from a monotonic counter.
  /// The same symbol can have different indexes in different bundles.
  /// This is is NOT the real address. This is to avoid disclosing KASLR through
  /// traces.
  #[prost(message, repeated, tag = "26")]
  pub kernel_symbols: ::prost::alloc::vec::Vec<InternedString>,
  /// Interned string values in the DebugAnnotation proto.
  #[prost(message, repeated, tag = "29")]
  pub debug_annotation_string_values: ::prost::alloc::vec::Vec<InternedString>,
  /// Interned protolog strings args.
  #[prost(message, repeated, tag = "36")]
  pub protolog_string_args: ::prost::alloc::vec::Vec<InternedString>,
  /// Interned protolog stacktraces.
  #[prost(message, repeated, tag = "37")]
  pub protolog_stacktrace: ::prost::alloc::vec::Vec<InternedString>,
  /// viewcapture
  #[prost(message, repeated, tag = "38")]
  pub viewcapture_package_name: ::prost::alloc::vec::Vec<InternedString>,
  #[prost(message, repeated, tag = "39")]
  pub viewcapture_window_name: ::prost::alloc::vec::Vec<InternedString>,
  #[prost(message, repeated, tag = "40")]
  pub viewcapture_view_id: ::prost::alloc::vec::Vec<InternedString>,
  #[prost(message, repeated, tag = "41")]
  pub viewcapture_class_name: ::prost::alloc::vec::Vec<InternedString>,
  /// Interned correlation ids in track_event.
  #[prost(message, repeated, tag = "43")]
  pub correlation_id_str: ::prost::alloc::vec::Vec<InternedString>,
}

/// Default values for TracePacket fields that hold for a particular TraceWriter
/// packet sequence. This message contains a subset of the TracePacket fields
/// with matching IDs. When provided, these fields define the default values
/// that should be applied, at import time, to all TracePacket(s) with the same
/// |trusted_packet_sequence_id|, unless otherwise specified in each packet.
///
/// Should be reemitted whenever incremental state is cleared on the sequence.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct TracePacketDefaults {
  #[prost(uint32, optional, tag = "58")]
  pub timestamp_clock_id: ::core::option::Option<u32>,
  /// Default values for TrackEvents (e.g. default track).
  #[prost(message, optional, tag = "11")]
  pub track_event_defaults: ::core::option::Option<TrackEventDefaults>,
}
/// Describes a process's attributes. Emitted as part of a TrackDescriptor,
/// usually by the process's main thread.
///
/// Next id: 9.
#[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
pub struct ProcessDescriptor {
  #[prost(int32, optional, tag = "1")]
  pub pid: ::core::option::Option<i32>,
  #[prost(string, repeated, tag = "2")]
  pub cmdline: ::prost::alloc::vec::Vec<::prost::alloc::string::String>,
  #[prost(string, optional, tag = "6")]
  pub process_name: ::core::option::Option<::prost::alloc::string::String>,
  #[prost(int32, optional, tag = "5")]
  pub process_priority: ::core::option::Option<i32>,
  /// Process start time in nanoseconds.
  /// The timestamp refers to the trace clock by default. Other clock IDs
  /// provided in TracePacket are not supported.
  #[prost(int64, optional, tag = "7")]
  pub start_timestamp_ns: ::core::option::Option<i64>,
  #[prost(
    enumeration = "process_descriptor::ChromeProcessType",
    optional,
    tag = "4"
  )]
  pub chrome_process_type: ::core::option::Option<i32>,
  /// To support old UI. New UI should determine default sorting by process_type.
  #[prost(int32, optional, tag = "3")]
  pub legacy_sort_index: ::core::option::Option<i32>,
  /// Labels can be used to further describe properties of the work performed by
  /// the process. For example, these can be used by Chrome renderer process to
  /// provide titles of frames being rendered.
  #[prost(string, repeated, tag = "8")]
  pub process_labels: ::prost::alloc::vec::Vec<::prost::alloc::string::String>,
}
/// Nested message and enum types in `ProcessDescriptor`.
pub mod process_descriptor {
  /// See chromium's content::ProcessType.
  #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
  #[repr(i32)]
  pub enum ChromeProcessType {
    ProcessUnspecified = 0,
    ProcessBrowser = 1,
    ProcessRenderer = 2,
    ProcessUtility = 3,
    ProcessZygote = 4,
    ProcessSandboxHelper = 5,
    ProcessGpu = 6,
    ProcessPpapiPlugin = 7,
    ProcessPpapiBroker = 8,
  }
  impl ChromeProcessType {
    /// String value of the enum field names used in the ProtoBuf definition.
    ///
    /// The values are not transformed in any way and thus are considered stable
    /// (if the ProtoBuf definition does not change) and safe for programmatic use.
    pub fn as_str_name(&self) -> &'static str {
      match self {
        Self::ProcessUnspecified => "PROCESS_UNSPECIFIED",
        Self::ProcessBrowser => "PROCESS_BROWSER",
        Self::ProcessRenderer => "PROCESS_RENDERER",
        Self::ProcessUtility => "PROCESS_UTILITY",
        Self::ProcessZygote => "PROCESS_ZYGOTE",
        Self::ProcessSandboxHelper => "PROCESS_SANDBOX_HELPER",
        Self::ProcessGpu => "PROCESS_GPU",
        Self::ProcessPpapiPlugin => "PROCESS_PPAPI_PLUGIN",
        Self::ProcessPpapiBroker => "PROCESS_PPAPI_BROKER",
      }
    }
    /// Creates an enum from field names used in the ProtoBuf definition.
    pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
      match value {
        "PROCESS_UNSPECIFIED" => Some(Self::ProcessUnspecified),
        "PROCESS_BROWSER" => Some(Self::ProcessBrowser),
        "PROCESS_RENDERER" => Some(Self::ProcessRenderer),
        "PROCESS_UTILITY" => Some(Self::ProcessUtility),
        "PROCESS_ZYGOTE" => Some(Self::ProcessZygote),
        "PROCESS_SANDBOX_HELPER" => Some(Self::ProcessSandboxHelper),
        "PROCESS_GPU" => Some(Self::ProcessGpu),
        "PROCESS_PPAPI_PLUGIN" => Some(Self::ProcessPpapiPlugin),
        "PROCESS_PPAPI_BROKER" => Some(Self::ProcessPpapiBroker),
        _ => None,
      }
    }
  }
}
/// This message specifies the "range of interest" for track events. With the
/// `drop_track_event_data_before` option set to `kTrackEventRangeOfInterest`,
/// Trace Processor drops track events outside of this range.
#[derive(Clone, Copy, PartialEq, Eq, Hash, ::prost::Message)]
pub struct TrackEventRangeOfInterest {
  #[prost(int64, optional, tag = "1")]
  pub start_us: ::core::option::Option<i64>,
}
/// Describes a thread's attributes. Emitted as part of a TrackDescriptor,
/// usually by the thread's trace writer.
///
/// Next id: 9.
#[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
pub struct ThreadDescriptor {
  #[prost(int32, optional, tag = "1")]
  pub pid: ::core::option::Option<i32>,
  #[prost(int32, optional, tag = "2")]
  pub tid: ::core::option::Option<i32>,
  #[prost(string, optional, tag = "5")]
  pub thread_name: ::core::option::Option<::prost::alloc::string::String>,
  #[prost(
    enumeration = "thread_descriptor::ChromeThreadType",
    optional,
    tag = "4"
  )]
  pub chrome_thread_type: ::core::option::Option<i32>,
  /// Deprecated. Use ClockSnapshot in combination with TracePacket's timestamp
  /// and timestamp_clock_id fields instead.
  #[prost(int64, optional, tag = "6")]
  pub reference_timestamp_us: ::core::option::Option<i64>,
  /// Absolute reference values. Clock values in subsequent TrackEvents can be
  /// encoded accumulatively and relative to these. This reduces their var-int
  /// encoding size.
  /// TODO(eseckler): Deprecated. Replace these with ClockSnapshot encoding.
  #[prost(int64, optional, tag = "7")]
  pub reference_thread_time_us: ::core::option::Option<i64>,
  #[prost(int64, optional, tag = "8")]
  pub reference_thread_instruction_count: ::core::option::Option<i64>,
  /// To support old UI. New UI should determine default sorting by thread_type.
  #[prost(int32, optional, tag = "3")]
  pub legacy_sort_index: ::core::option::Option<i32>,
}
/// Nested message and enum types in `ThreadDescriptor`.
pub mod thread_descriptor {
  #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
  #[repr(i32)]
  pub enum ChromeThreadType {
    ChromeThreadUnspecified = 0,
    ChromeThreadMain = 1,
    ChromeThreadIo = 2,
    /// Scheduler:
    ChromeThreadPoolBgWorker = 3,
    ChromeThreadPoolFgWorker = 4,
    ChromeThreadPoolFbBlocking = 5,
    ChromeThreadPoolBgBlocking = 6,
    ChromeThreadPoolService = 7,
    /// Compositor:
    ChromeThreadCompositor = 8,
    ChromeThreadVizCompositor = 9,
    ChromeThreadCompositorWorker = 10,
    /// Renderer:
    ChromeThreadServiceWorker = 11,
    /// Tracing related threads:
    ChromeThreadMemoryInfra = 50,
    ChromeThreadSamplingProfiler = 51,
  }
  impl ChromeThreadType {
    /// String value of the enum field names used in the ProtoBuf definition.
    ///
    /// The values are not transformed in any way and thus are considered stable
    /// (if the ProtoBuf definition does not change) and safe for programmatic use.
    pub fn as_str_name(&self) -> &'static str {
      match self {
        Self::ChromeThreadUnspecified => "CHROME_THREAD_UNSPECIFIED",
        Self::ChromeThreadMain => "CHROME_THREAD_MAIN",
        Self::ChromeThreadIo => "CHROME_THREAD_IO",
        Self::ChromeThreadPoolBgWorker => "CHROME_THREAD_POOL_BG_WORKER",
        Self::ChromeThreadPoolFgWorker => "CHROME_THREAD_POOL_FG_WORKER",
        Self::ChromeThreadPoolFbBlocking => "CHROME_THREAD_POOL_FB_BLOCKING",
        Self::ChromeThreadPoolBgBlocking => "CHROME_THREAD_POOL_BG_BLOCKING",
        Self::ChromeThreadPoolService => "CHROME_THREAD_POOL_SERVICE",
        Self::ChromeThreadCompositor => "CHROME_THREAD_COMPOSITOR",
        Self::ChromeThreadVizCompositor => "CHROME_THREAD_VIZ_COMPOSITOR",
        Self::ChromeThreadCompositorWorker => "CHROME_THREAD_COMPOSITOR_WORKER",
        Self::ChromeThreadServiceWorker => "CHROME_THREAD_SERVICE_WORKER",
        Self::ChromeThreadMemoryInfra => "CHROME_THREAD_MEMORY_INFRA",
        Self::ChromeThreadSamplingProfiler => "CHROME_THREAD_SAMPLING_PROFILER",
      }
    }
    /// Creates an enum from field names used in the ProtoBuf definition.
    pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
      match value {
        "CHROME_THREAD_UNSPECIFIED" => Some(Self::ChromeThreadUnspecified),
        "CHROME_THREAD_MAIN" => Some(Self::ChromeThreadMain),
        "CHROME_THREAD_IO" => Some(Self::ChromeThreadIo),
        "CHROME_THREAD_POOL_BG_WORKER" => Some(Self::ChromeThreadPoolBgWorker),
        "CHROME_THREAD_POOL_FG_WORKER" => Some(Self::ChromeThreadPoolFgWorker),
        "CHROME_THREAD_POOL_FB_BLOCKING" => Some(Self::ChromeThreadPoolFbBlocking),
        "CHROME_THREAD_POOL_BG_BLOCKING" => Some(Self::ChromeThreadPoolBgBlocking),
        "CHROME_THREAD_POOL_SERVICE" => Some(Self::ChromeThreadPoolService),
        "CHROME_THREAD_COMPOSITOR" => Some(Self::ChromeThreadCompositor),
        "CHROME_THREAD_VIZ_COMPOSITOR" => Some(Self::ChromeThreadVizCompositor),
        "CHROME_THREAD_COMPOSITOR_WORKER" => Some(Self::ChromeThreadCompositorWorker),
        "CHROME_THREAD_SERVICE_WORKER" => Some(Self::ChromeThreadServiceWorker),
        "CHROME_THREAD_MEMORY_INFRA" => Some(Self::ChromeThreadMemoryInfra),
        "CHROME_THREAD_SAMPLING_PROFILER" => Some(Self::ChromeThreadSamplingProfiler),
        _ => None,
      }
    }
  }
}
/// Describes the attributes for a Chrome process. Must be paired with a
/// ProcessDescriptor in the same TrackDescriptor.
///
/// Next id: 6.
#[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
pub struct ChromeProcessDescriptor {
  /// This is a chrome_enums::ProcessType from
  /// //protos/third_party/chromium/chrome_enums.proto. The enum definition can't
  /// be imported here because of a dependency loop.
  #[prost(int32, optional, tag = "1")]
  pub process_type: ::core::option::Option<i32>,
  #[prost(int32, optional, tag = "2")]
  pub process_priority: ::core::option::Option<i32>,
  /// To support old UI. New UI should determine default sorting by process_type.
  #[prost(int32, optional, tag = "3")]
  pub legacy_sort_index: ::core::option::Option<i32>,
  /// Name of the hosting app for WebView. Used to match renderer processes to
  /// their hosting apps.
  #[prost(string, optional, tag = "4")]
  pub host_app_package_name: ::core::option::Option<::prost::alloc::string::String>,
  /// The ID to link crashes to trace.
  /// Notes:
  /// * The ID is per process. So, each trace may contain many IDs, and you need
  ///    to look for the ID from crashed process to find the crash report.
  /// * Having a "chrome-trace-id" in crash doesn't necessarily mean we can
  ///    get an uploaded trace, since uploads could have failed.
  /// * On the other hand, if there was a crash during the session and trace was
  ///    uploaded, it is very likely to find a crash report with the trace ID.
  /// * This is not crash ID or trace ID. It is just a random 64-bit number
  ///    recorded in both traces and crashes. It is possible to have collisions,
  ///    though very rare.
  #[prost(uint64, optional, tag = "5")]
  pub crash_trace_id: ::core::option::Option<u64>,
}
/// Describes a Chrome thread's attributes. Emitted as part of a TrackDescriptor,
/// usually by the thread's trace writer. Must be paired with a ThreadDescriptor
/// in the same TrackDescriptor.
///
/// Next id: 3.
#[derive(Clone, Copy, PartialEq, Eq, Hash, ::prost::Message)]
pub struct ChromeThreadDescriptor {
  /// This is a chrome_enums::ThreadType from
  /// //protos/third_party/chromium/chrome_enums.proto. The enum definition can't
  /// be imported here because of a dependency loop.
  #[prost(int32, optional, tag = "1")]
  pub thread_type: ::core::option::Option<i32>,
  /// To support old UI. New UI should determine default sorting by thread_type.
  #[prost(int32, optional, tag = "2")]
  pub legacy_sort_index: ::core::option::Option<i32>,
  /// Indicates whether the thread's tid specified in the thread descriptor is
  /// namespaced by Chromium's sandbox. Only set on Linux, and from Chrome M140.
  #[prost(bool, optional, tag = "3")]
  pub is_sandboxed_tid: ::core::option::Option<bool>,
}
/// Defines properties of a counter track, e.g. for built-in counters (thread
/// time, instruction count, ..) or user-specified counters (e.g. memory usage of
/// a specific app component).
///
/// Counter tracks only support TYPE_COUNTER track events, which specify new
/// values for the counter. For counters that require per-slice values, counter
/// values can instead be provided in a more efficient encoding via TrackEvent's
/// |extra_counter_track_uuids| and |extra_counter_values| fields. However,
/// slice-type events cannot be emitted onto a counter track.
///
/// Values for counters that are only emitted on a single packet sequence can
/// optionally be delta-encoded, see |is_incremental|.
///
/// Next id: 7.
#[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
pub struct CounterDescriptor {
  /// For built-in counters (e.g. thread time). Custom user-specified counters
  /// (e.g. those emitted by TRACE_COUNTER macros of the client library)
  /// shouldn't set this, and instead provide a counter name via TrackDescriptor.
  #[prost(
    enumeration = "counter_descriptor::BuiltinCounterType",
    optional,
    tag = "1"
  )]
  pub r#type: ::core::option::Option<i32>,
  /// Names of categories of the counter (usually for user-specified counters).
  /// In the client library, categories are a way to turn groups of individual
  /// counters (or events) on or off.
  #[prost(string, repeated, tag = "2")]
  pub categories: ::prost::alloc::vec::Vec<::prost::alloc::string::String>,
  /// Type of the counter's values. Built-in counters imply a value for this
  /// field.
  #[prost(enumeration = "counter_descriptor::Unit", optional, tag = "3")]
  pub unit: ::core::option::Option<i32>,
  /// In order to use a unit not defined as a part of |Unit|, a free-form unit
  /// name can be used instead.
  #[prost(string, optional, tag = "6")]
  pub unit_name: ::core::option::Option<::prost::alloc::string::String>,
  /// Multiplication factor of this counter's values, e.g. to supply
  /// COUNTER_THREAD_TIME_NS timestamps in microseconds instead.
  #[prost(int64, optional, tag = "4")]
  pub unit_multiplier: ::core::option::Option<i64>,
  /// Whether values for this counter are provided as delta values. Only
  /// supported for counters that are emitted on a single packet-sequence (e.g.
  /// thread time). Counter values in subsequent packets on the current packet
  /// sequence will be interpreted as delta values from the sequence's most
  /// recent value for the counter. When incremental state is cleared, the
  /// counter value is considered to be reset to 0. Thus, the first value after
  /// incremental state is cleared is effectively an absolute value.
  #[prost(bool, optional, tag = "5")]
  pub is_incremental: ::core::option::Option<bool>,
  /// When visualizing multiple counter tracks, it is often useful to have them
  /// share the same Y-axis range. This allows for easy comparison of their
  /// values.
  ///
  /// All counter tracks with the same |y_axis_share_key| and the same parent
  /// track (e.g. grouped under the same process track) will share their y-axis
  /// range in the UI.
  #[prost(string, optional, tag = "7")]
  pub y_axis_share_key: ::core::option::Option<::prost::alloc::string::String>,
}
/// Nested message and enum types in `CounterDescriptor`.
pub mod counter_descriptor {
  /// Built-in counters, usually with special meaning in the client library,
  /// trace processor, legacy JSON format, or UI. Trace processor will infer a
  /// track name from the enum value if none is provided in TrackDescriptor.
  #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
  #[repr(i32)]
  pub enum BuiltinCounterType {
    CounterUnspecified = 0,
    /// implies UNIT_TIME_NS.
    CounterThreadTimeNs = 1,
    /// implies UNIT_COUNT.
    CounterThreadInstructionCount = 2,
  }
  impl BuiltinCounterType {
    /// String value of the enum field names used in the ProtoBuf definition.
    ///
    /// The values are not transformed in any way and thus are considered stable
    /// (if the ProtoBuf definition does not change) and safe for programmatic use.
    pub fn as_str_name(&self) -> &'static str {
      match self {
        Self::CounterUnspecified => "COUNTER_UNSPECIFIED",
        Self::CounterThreadTimeNs => "COUNTER_THREAD_TIME_NS",
        Self::CounterThreadInstructionCount => "COUNTER_THREAD_INSTRUCTION_COUNT",
      }
    }
    /// Creates an enum from field names used in the ProtoBuf definition.
    pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
      match value {
        "COUNTER_UNSPECIFIED" => Some(Self::CounterUnspecified),
        "COUNTER_THREAD_TIME_NS" => Some(Self::CounterThreadTimeNs),
        "COUNTER_THREAD_INSTRUCTION_COUNT" => Some(Self::CounterThreadInstructionCount),
        _ => None,
      }
    }
  }
  /// Type of the values for the counters - to supply lower granularity units,
  /// see also |unit_multiplier|.
  #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
  #[repr(i32)]
  pub enum Unit {
    Unspecified = 0,
    TimeNs = 1,
    Count = 2,
    /// TODO(eseckler): Support more units as necessary.
    SizeBytes = 3,
  }
  impl Unit {
    /// String value of the enum field names used in the ProtoBuf definition.
    ///
    /// The values are not transformed in any way and thus are considered stable
    /// (if the ProtoBuf definition does not change) and safe for programmatic use.
    pub fn as_str_name(&self) -> &'static str {
      match self {
        Self::Unspecified => "UNIT_UNSPECIFIED",
        Self::TimeNs => "UNIT_TIME_NS",
        Self::Count => "UNIT_COUNT",
        Self::SizeBytes => "UNIT_SIZE_BYTES",
      }
    }
    /// Creates an enum from field names used in the ProtoBuf definition.
    pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
      match value {
        "UNIT_UNSPECIFIED" => Some(Self::Unspecified),
        "UNIT_TIME_NS" => Some(Self::TimeNs),
        "UNIT_COUNT" => Some(Self::Count),
        "UNIT_SIZE_BYTES" => Some(Self::SizeBytes),
        _ => None,
      }
    }
  }
}
/// Defines a track for TrackEvents. Slices and instant events on the same track
/// will be nested based on their timestamps, see TrackEvent::Type.
///
/// A TrackDescriptor only needs to be emitted by one trace writer / producer and
/// is valid for the entirety of the trace. To ensure the descriptor isn't lost
/// when the ring buffer wraps, it should be reemitted whenever incremental state
/// is cleared.
///
/// As a fallback, TrackEvents emitted without an explicit track association will
/// be associated with an implicit trace-global track (uuid = 0), see also
/// |TrackEvent::track_uuid|. It is possible but not necessary to emit a
/// TrackDescriptor for this implicit track.
///
/// Next id: 18.
#[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
pub struct TrackDescriptor {
  /// Unique ID that identifies this track. This ID is global to the whole trace.
  /// Producers should ensure that it is unlikely to clash with IDs emitted by
  /// other producers. A value of 0 denotes the implicit trace-global track.
  ///
  /// For example, legacy TRACE_EVENT macros may use a hash involving the async
  /// event id + id_scope, pid, and/or tid to compute this ID.
  #[prost(uint64, optional, tag = "1")]
  pub uuid: ::core::option::Option<u64>,
  /// A parent track reference can be used to describe relationships between
  /// tracks. For example, to define an asynchronous track which is scoped to a
  /// specific process, specify the uuid for that process's process track here.
  /// Similarly, to associate a COUNTER_THREAD_TIME_NS counter track with a
  /// thread, specify the uuid for that thread's thread track here. In general,
  /// setting a parent will *nest* that track under the parent in the UI and in
  /// the trace processor data model (with the important exception noted below).
  ///
  /// If not specified, the track will be a root track, i.e. not nested under any
  /// other track.
  ///
  /// Note: if the `thread` or `process` fields are set, this value will be
  /// *ignored* as priority is given to those fields.
  ///
  /// Note: if the parent is set to a track with `thread` or `process` fields
  /// set, the track will *not* be nested under the parent in the UI and in the
  /// trace processor data model. Instead, the track will inherit the parent's
  /// thread/process association and will appear as a *sibling* of the parent.
  /// This semantic exists for back-compat reasons as the UI used to work this
  /// way for years and changing this leads to a lot of traces subtly breaking.
  /// If you want to force nesting, create *another* intermediate track to act as
  /// the parent.
  #[prost(uint64, optional, tag = "5")]
  pub parent_uuid: ::core::option::Option<u64>,
  /// A human-readable description of the track providing more context about its
  /// data. In the UI, this is shown in a popup when the track's help button is
  /// clicked.
  #[prost(string, optional, tag = "14")]
  pub description: ::core::option::Option<::prost::alloc::string::String>,
  /// Associate the track with a process, making it the process-global track.
  /// There should only be one such track per process (usually for instant
  /// events; trace processor uses this fact to detect pid reuse). If you need
  /// more (e.g. for asynchronous events), create child tracks using parent_uuid.
  ///
  /// Trace processor will merge events on a process track with slice-type events
  /// from other sources (e.g. ftrace) for the same process into a single
  /// timeline view.
  #[prost(message, optional, tag = "3")]
  pub process: ::core::option::Option<ProcessDescriptor>,
  #[prost(message, optional, tag = "6")]
  pub chrome_process: ::core::option::Option<ChromeProcessDescriptor>,
  /// Associate the track with a thread, indicating that the track's events
  /// describe synchronous code execution on the thread. There should only be one
  /// such track per thread (trace processor uses this fact to detect tid reuse).
  ///
  /// Trace processor will merge events on a thread track with slice-type events
  /// from other sources (e.g. ftrace) for the same thread into a single timeline
  /// view.
  #[prost(message, optional, tag = "4")]
  pub thread: ::core::option::Option<ThreadDescriptor>,
  #[prost(message, optional, tag = "7")]
  pub chrome_thread: ::core::option::Option<ChromeThreadDescriptor>,
  /// Descriptor for a counter track. If set, the track will only support
  /// TYPE_COUNTER TrackEvents (and values provided via TrackEvent's
  /// |extra_counter_values|).
  #[prost(message, optional, tag = "8")]
  pub counter: ::core::option::Option<CounterDescriptor>,
  /// If true, forces Trace Processor to use separate tracks for track events
  /// and system events for the same thread.
  ///
  /// Track events timestamps in Chrome have microsecond resolution, while
  /// system events use nanoseconds. It results in broken event nesting when
  /// track events and system events share a track.
  #[prost(bool, optional, tag = "9")]
  pub disallow_merging_with_system_tracks: ::core::option::Option<bool>,
  #[prost(
    enumeration = "track_descriptor::ChildTracksOrdering",
    optional,
    tag = "11"
  )]
  pub child_ordering: ::core::option::Option<i32>,
  /// An opaque value which allows specifying how two sibling tracks should be
  /// ordered relative to each other: tracks with lower ranks will appear before
  /// tracks with higher ranks. An unspecified rank will be treated as a rank of
  /// 0.
  ///
  /// Note: this option is only relevant for tracks where the parent has
  /// `child_ordering` set to `EXPLICIT`. It is ignored otherwise.
  ///
  /// Note: for tracks where the parent has `thread` or `process` are set, this
  /// option is *ignored* (even if the parent's `child_ordering` is `EXPLICIT``).
  /// See `parent_uuid` for details.
  #[prost(int32, optional, tag = "12")]
  pub sibling_order_rank: ::core::option::Option<i32>,
  #[prost(
    enumeration = "track_descriptor::SiblingMergeBehavior",
    optional,
    tag = "15"
  )]
  pub sibling_merge_behavior: ::core::option::Option<i32>,
  /// Name of the track.
  ///
  /// Optional but *strongly recommended* to be specified in a `TrackDescriptor`
  /// emitted before any `TrackEvent`s on the same track.
  ///
  /// Note: any name specified here will be *ignored* for the root thread scoped
  /// tracks when `disallow_merging_with_system_tracks` is not set, as in this
  /// case, the name of the track is shared by many different data sources and so
  /// is centrally controlled by trace processor.
  ///
  /// It's strongly recommended to only emit the name for a track uuid *once*. If
  /// a descriptor *has* to be emitted multiple times (e.g. between different
  /// processes), it's recommended to ensure that the name is consistent across
  /// all TrackDescriptors with the same `uuid`.
  ///
  /// If the the above recommendation is not followed and the same uuid is
  /// emitted with different names, it is implementation defined how the final
  /// name will be chosen and may change at any time.
  ///
  /// The current implementation of trace processor chooses the name in the
  /// following way, depending on the value of the `sibling_merge_behavior`
  /// field:
  ///
  /// 1. If `sibling_merge_behavior` is set to `SIBLING_MERGE_BEHAVIOR_NONE`:
  ///    * The *last* non-null name in the whole trace according to trace order
  ///      will be used.
  ///    * If no non-null name is present in the whole trace, the trace processor
  ///      may fall back to other sources to provide a name for the track (e.g.
  ///      the first event name for slice tracks, the counter name for counter
  ///      tracks). This is implementation defined and may change at any time.
  ///
  /// 2. If `sibling_merge_behavior` is set to any other value:
  ///    * The first non-null name before the first event on the track *or on any
  ///      descendant tracks* is processed will be used. For example, consider
  ///      the following sequence of events:
  ///        ts=100: TrackDescriptor(uuid=A)
  ///        ts=200: TrackDescriptor(uuid=B, parent_uuid=A)
  ///        ts=300: TrackDescriptor(uuid=A, name="Track A")
  ///        ts=400: TrackEvent(track_uuid=B)
  ///      In this case, the name for track A will be "Track A" because the
  ///      descriptor with the name was emitted before the first event on a
  ///      descendant track (B).
  ///    * If no non-null name is present before the event is processed, the trace
  ///      processor may fall back to other sources to provide a name for the
  ///      track (e.g. the first event name for slice tracks, the counter name for
  ///      counter tracks). This is implementation defined and may change at any
  ///      time.
  ///    * Note on processing order: In the standard trace processor pipeline,
  ///      `TrackDescriptor`s are processed during a "tokenization" phase, which
  ///      occurs before any `TrackEvent`s are parsed. This means that for a given
  ///      track, all its descriptors in the trace are processed before its
  ///      events. Consequently, the "first non-null name before the first event"
  ///      will be the name from the first `TrackDescriptor` for that track in the
  ///      trace file that has a non-null name. However, in a streaming parsing
  ///      scenario, the timestamp order of descriptors and events is significant,
  ///      and a descriptor arriving after an event has been processed will not be
  ///      used to name the track.
  #[prost(oneof = "track_descriptor::StaticOrDynamicName", tags = "2, 10, 13")]
  pub static_or_dynamic_name: ::core::option::Option<track_descriptor::StaticOrDynamicName>,
  /// An opaque value which allows specifying which tracks should be merged
  /// together.
  ///
  /// Only meaningful when `sibling_merge_behavior` is set to
  /// `SIBLING_MERGE_BEHAVIOR_BY_SIBLING_MERGE_KEY`.
  #[prost(oneof = "track_descriptor::SiblingMergeKeyField", tags = "16, 17")]
  pub sibling_merge_key_field: ::core::option::Option<track_descriptor::SiblingMergeKeyField>,
}
/// Nested message and enum types in `TrackDescriptor`.
pub mod track_descriptor {
  /// Specifies how the UI should display child tracks of this track (i.e. tracks
  /// where `parent_uuid` is specified to this track `uuid`). Note that this
  /// value is simply a *hint* to the UI: the UI is not guarnateed to respect
  /// this if it has a good reason not to do so.
  ///
  /// Note: for tracks where `thread` or `process` are set, this option is
  /// *ignored*. See `parent_uuid` for details.
  #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
  #[repr(i32)]
  pub enum ChildTracksOrdering {
    /// The default ordering, with no bearing on how the UI will visualise the
    /// tracks.
    Unknown = 0,
    /// Order tracks by `name` or `static_name` depending on which one has been
    /// specified.
    Lexicographic = 1,
    /// Order tracks by the first `ts` event in a track.
    Chronological = 2,
    /// Order tracks by `sibling_order_rank` of child tracks. Child tracks with
    /// the lower values will be shown before tracks with higher values. Tracks
    /// with no value will be treated as having 0 rank.
    Explicit = 3,
  }
  impl ChildTracksOrdering {
    /// String value of the enum field names used in the ProtoBuf definition.
    ///
    /// The values are not transformed in any way and thus are considered stable
    /// (if the ProtoBuf definition does not change) and safe for programmatic use.
    pub fn as_str_name(&self) -> &'static str {
      match self {
        Self::Unknown => "UNKNOWN",
        Self::Lexicographic => "LEXICOGRAPHIC",
        Self::Chronological => "CHRONOLOGICAL",
        Self::Explicit => "EXPLICIT",
      }
    }
    /// Creates an enum from field names used in the ProtoBuf definition.
    pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
      match value {
        "UNKNOWN" => Some(Self::Unknown),
        "LEXICOGRAPHIC" => Some(Self::Lexicographic),
        "CHRONOLOGICAL" => Some(Self::Chronological),
        "EXPLICIT" => Some(Self::Explicit),
        _ => None,
      }
    }
  }
  /// Specifies how the analysis tools should "merge" different sibling
  /// TrackEvent tracks.
  ///
  /// For two or more tracks to be merged, they must be "eligible" siblings.
  /// Eligibility is determined by the following rules:
  /// 1. All tracks must have the same parent.
  /// 2. All tracks must have the same `sibling_merge_behavior`. The only
  ///     exception is `SIBLING_MERGE_BEHAVIOR_UNSPECIFIED` which is treated as
  ///     `SIBLING_MERGE_BEHAVIOR_BY_TRACK_NAME`.
  /// 3. Depending on the behavior, the corresponding key must match (e.g. `name`
  ///     for `BY_TRACK_NAME`, `sibling_merge_key` for `BY_SIBLING_MERGE_KEY`).
  ///
  /// Specifically:
  ///    - in the UI, all tracks which are merged together will be
  ///      displayed as a single "visual" track.
  ///    - in the trace processor, all tracks which are merged together will be
  ///      "multiplexed" into n "analysis" tracks where n is the maximum number
  ///      of tracks which have an active event at the same time.
  ///
  /// When tracks are merged togther, the properties for the merged track will be
  /// chosen from the source tracks based on the following rules:
  ///    - for `sibling_order_rank`: the rank of the merged track will be the
  ///      smallest rank among the source tracks.
  ///    - for all other properties: the property taken is unspecified and can
  ///      be any value provided by one of the source tracks. This can lead to
  ///      non-deterministic behavior.
  ///       - examples of other properties include `name`, `child_ordering` etc.
  ///       - because of this, it's strongly recommended to ensure that all source
  ///         tracks have the same value for these properties.
  ///       - the trace processor will also emit an error stat if it detects
  ///         that the properties are not the same across all source tracks.
  ///
  /// Note: merging is done *recursively* so entire trees of tracks can be merged
  /// together. To make this clearer, consider an example track hierarchy (in
  /// the diagrams: "smk" refers to "sibling_merge_key", the first word on a
  /// track line, like "Updater", is its 'name' property):
  ///
  ///    Initial track hierarchy:
  ///      SystemActivity
  ///      ├── AuthService (smk: "auth_main_cluster")
  ///      │   └── LoginOp (smk: "login_v1")
  ///      ├── AuthService (smk: "auth_main_cluster")
  ///      │   └── LoginOp (smk: "login_v1")
  ///      ├── AuthService (smk: "auth_backup_cluster")
  ///      │   └── GuestOp (smk: "guest_v1")
  ///      └── UserProfileService (smk: "profile_cluster")
  ///          └── GetProfileOp (smk: "getprofile_v1")
  ///
  /// Merging outcomes:
  ///
  /// Scenario 1: Merging by `SIBLING_MERGE_BEHAVIOR_BY_SIBLING_MERGE_KEY`
  ///    - The first two "AuthService" tracks merge because they share
  ///      `smk: "auth_main_cluster"`. Their names are consistent ("AuthService"),
  ///      aligning with recommendations. The merged track is named "AuthService".
  ///    - The third "AuthService" track (with `smk: "auth_backup_cluster"`)
  ///      remains separate, as its `sibling_merge_key` is different.
  ///    - "UserProfileService" also remains separate.
  ///    - Within the merged "AuthService" (from "auth_main_cluster"):
  ///      "LoginOp" get merged as they have the same sibling merge key.
  ///
  ///    Resulting UI (when merging by SIBLING_MERGE_KEY):
  ///      SystemActivity
  ///      ├── AuthService (merged by smk: "auth_main_cluster")
  ///      │   ├── LoginOp (merged by smk: "login_v1")
  ///      ├── AuthService (smk: "auth_backup_cluster")
  ///      │   └── GuestOp (smk: "guest_v1")
  ///      └── UserProfileService (smk: "profile_cluster")
  ///          └── GetProfileOp (smk: "getprofile_v1")
  ///
  /// Scenario 2: Merging by `SIBLING_MERGE_BEHAVIOR_BY_TRACK_NAME`
  ///    - All three tracks named "AuthService" merge because they share the same
  ///      name. The merged track is named "AuthService". The `sibling_merge_key`
  ///      for this merged track would be taken from one of the source tracks
  ///      (e.g., "auth_main_cluster" or "auth_backup_cluster"), which could be
  ///      relevant if its children had key-based merge behaviors.
  ///    - "UserProfileService" remains separate due to its different name.
  ///    - Within the single merged "AuthService" track:
  ///      "LoginOp", "GuestOp" become siblings. "LoginOp" tracks gets merged as
  ///      they have the same name.
  ///
  ///    Resulting UI (when merging by SIBLING_MERGE_BEHAVIOR_BY_TRACK_NAME):
  ///      SystemActivity
  ///      ├── AuthService (merged from 3 "AuthService" tracks)
  ///      │   ├── LoginOp (smk: "login_v1")
  ///      │   └── GuestOp (smk: "guest_v1")
  ///      └── UserProfileService (smk: "profile_cluster")
  ///          └── GetProfileOp (smk: "getprofile_v1")
  ///
  /// Note: for tracks where `thread` or `process` are set, this option is
  /// *ignored*. See `parent_uuid` for details.
  #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
  #[repr(i32)]
  pub enum SiblingMergeBehavior {
    /// When unspecified or not set, defaults to
    /// `SIBLING_MERGE_BEHAVIOR_BY_TRACK_NAME`.
    Unspecified = 0,
    /// Merge this track with eligible siblings which have the same `name`.
    ///
    /// This is the default behavior.option.
    ///
    /// Fun fact: this is the default beahavior for legacy reasons as the UI has
    /// worked this way for years and inherited this behavior from
    /// chrome://tracing which has worked this way for even longer
    ByTrackName = 1,
    /// Never merge this track with any siblings. Useful if if this track has a
    /// specific meaning and you want to see separately from any others.
    None = 2,
    /// Merge this track with eligible siblings which have the same
    /// `sibling_merge_key`.
    BySiblingMergeKey = 3,
  }
  impl SiblingMergeBehavior {
    /// String value of the enum field names used in the ProtoBuf definition.
    ///
    /// The values are not transformed in any way and thus are considered stable
    /// (if the ProtoBuf definition does not change) and safe for programmatic use.
    pub fn as_str_name(&self) -> &'static str {
      match self {
        Self::Unspecified => "SIBLING_MERGE_BEHAVIOR_UNSPECIFIED",
        Self::ByTrackName => "SIBLING_MERGE_BEHAVIOR_BY_TRACK_NAME",
        Self::None => "SIBLING_MERGE_BEHAVIOR_NONE",
        Self::BySiblingMergeKey => "SIBLING_MERGE_BEHAVIOR_BY_SIBLING_MERGE_KEY",
      }
    }
    /// Creates an enum from field names used in the ProtoBuf definition.
    pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
      match value {
        "SIBLING_MERGE_BEHAVIOR_UNSPECIFIED" => Some(Self::Unspecified),
        "SIBLING_MERGE_BEHAVIOR_BY_TRACK_NAME" => Some(Self::ByTrackName),
        "SIBLING_MERGE_BEHAVIOR_NONE" => Some(Self::None),
        "SIBLING_MERGE_BEHAVIOR_BY_SIBLING_MERGE_KEY" => Some(Self::BySiblingMergeKey),
        _ => None,
      }
    }
  }
  /// Name of the track.
  ///
  /// Optional but *strongly recommended* to be specified in a `TrackDescriptor`
  /// emitted before any `TrackEvent`s on the same track.
  ///
  /// Note: any name specified here will be *ignored* for the root thread scoped
  /// tracks when `disallow_merging_with_system_tracks` is not set, as in this
  /// case, the name of the track is shared by many different data sources and so
  /// is centrally controlled by trace processor.
  ///
  /// It's strongly recommended to only emit the name for a track uuid *once*. If
  /// a descriptor *has* to be emitted multiple times (e.g. between different
  /// processes), it's recommended to ensure that the name is consistent across
  /// all TrackDescriptors with the same `uuid`.
  ///
  /// If the the above recommendation is not followed and the same uuid is
  /// emitted with different names, it is implementation defined how the final
  /// name will be chosen and may change at any time.
  ///
  /// The current implementation of trace processor chooses the name in the
  /// following way, depending on the value of the `sibling_merge_behavior`
  /// field:
  ///
  /// 1. If `sibling_merge_behavior` is set to `SIBLING_MERGE_BEHAVIOR_NONE`:
  ///    * The *last* non-null name in the whole trace according to trace order
  ///      will be used.
  ///    * If no non-null name is present in the whole trace, the trace processor
  ///      may fall back to other sources to provide a name for the track (e.g.
  ///      the first event name for slice tracks, the counter name for counter
  ///      tracks). This is implementation defined and may change at any time.
  ///
  /// 2. If `sibling_merge_behavior` is set to any other value:
  ///    * The first non-null name before the first event on the track *or on any
  ///      descendant tracks* is processed will be used. For example, consider
  ///      the following sequence of events:
  ///        ts=100: TrackDescriptor(uuid=A)
  ///        ts=200: TrackDescriptor(uuid=B, parent_uuid=A)
  ///        ts=300: TrackDescriptor(uuid=A, name="Track A")
  ///        ts=400: TrackEvent(track_uuid=B)
  ///      In this case, the name for track A will be "Track A" because the
  ///      descriptor with the name was emitted before the first event on a
  ///      descendant track (B).
  ///    * If no non-null name is present before the event is processed, the trace
  ///      processor may fall back to other sources to provide a name for the
  ///      track (e.g. the first event name for slice tracks, the counter name for
  ///      counter tracks). This is implementation defined and may change at any
  ///      time.
  ///    * Note on processing order: In the standard trace processor pipeline,
  ///      `TrackDescriptor`s are processed during a "tokenization" phase, which
  ///      occurs before any `TrackEvent`s are parsed. This means that for a given
  ///      track, all its descriptors in the trace are processed before its
  ///      events. Consequently, the "first non-null name before the first event"
  ///      will be the name from the first `TrackDescriptor` for that track in the
  ///      trace file that has a non-null name. However, in a streaming parsing
  ///      scenario, the timestamp order of descriptors and events is significant,
  ///      and a descriptor arriving after an event has been processed will not be
  ///      used to name the track.
  #[derive(Clone, PartialEq, Eq, Hash, ::prost::Oneof)]
  pub enum StaticOrDynamicName {
    #[prost(string, tag = "2")]
    Name(::prost::alloc::string::String),
    /// This field is only set by the SDK when perfetto::StaticString is
    /// provided.
    #[prost(string, tag = "10")]
    StaticName(::prost::alloc::string::String),
    /// Equivalent to name, used just to mark that the data is coming from
    /// android.os.Trace.
    #[prost(string, tag = "13")]
    AtraceName(::prost::alloc::string::String),
  }
  /// An opaque value which allows specifying which tracks should be merged
  /// together.
  ///
  /// Only meaningful when `sibling_merge_behavior` is set to
  /// `SIBLING_MERGE_BEHAVIOR_BY_SIBLING_MERGE_KEY`.
  #[derive(Clone, PartialEq, Eq, Hash, ::prost::Oneof)]
  pub enum SiblingMergeKeyField {
    #[prost(string, tag = "16")]
    SiblingMergeKey(::prost::alloc::string::String),
    #[prost(uint64, tag = "17")]
    SiblingMergeKeyInt(u64),
  }
}

/// A random unique ID that identifies the trace.
/// This message has been introduced in v32. Prior to that, the UUID was
/// only (optionally) present in the TraceConfig.trace_uuid_msb/lsb fields.
/// This has been moved to a standalone packet to deal with new use-cases for
/// go/gapless-aot, where the same tracing session can be serialized several
/// times, in which case the UUID is changed on each snapshot and does not match
/// the one in the TraceConfig.
#[derive(Clone, Copy, PartialEq, Eq, Hash, ::prost::Message)]
pub struct TraceUuid {
  #[prost(int64, optional, tag = "1")]
  pub msb: ::core::option::Option<i64>,
  #[prost(int64, optional, tag = "2")]
  pub lsb: ::core::option::Option<i64>,
}
/// TracePacket is the root object of a Perfetto trace.
/// A Perfetto trace is a linear sequence of TracePacket(s).
///
/// The tracing service guarantees that all TracePacket(s) written by a given
/// TraceWriter are seen in-order, without gaps or duplicates. If, for any
/// reason, a TraceWriter sequence becomes invalid, no more packets are returned
/// to the Consumer (or written into the trace file).
/// TracePacket(s) written by different TraceWriter(s), hence even different
/// data sources, can be seen in arbitrary order.
/// The consumer can re-establish a total order, if interested, using the packet
/// timestamps, after having synchronized the different clocks onto a global
/// clock.
///
/// The tracing service is agnostic of the content of TracePacket, with the
/// exception of few fields (e.g.. trusted_*, trace_config) that are written by
/// the service itself.
///
/// See the [Buffers and Dataflow](/docs/concepts/buffers.md) doc for details.
///
/// Next reserved id: 14 (up to 15).
/// Next id: 124.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct TracePacket {
  /// The timestamp of the TracePacket.
  /// By default this timestamps refers to the trace clock (CLOCK_BOOTTIME on
  /// Android). It can be overridden using a different timestamp_clock_id.
  /// The clock domain definition in ClockSnapshot can also override:
  /// - The unit (default: 1ns).
  /// - The absolute vs delta encoding (default: absolute timestamp).
  #[prost(uint64, optional, tag = "8")]
  pub timestamp: ::core::option::Option<u64>,
  /// Specifies the ID of the clock used for the TracePacket |timestamp|. Can be
  /// one of the built-in types from ClockSnapshot::BuiltinClocks, or a
  /// producer-defined clock id.
  /// If unspecified and if no default per-sequence value has been provided via
  /// TracePacketDefaults, it defaults to BuiltinClocks::BOOTTIME.
  #[prost(uint32, optional, tag = "58")]
  pub timestamp_clock_id: ::core::option::Option<u32>,
  /// Trusted process id of the producer which generated this packet, written by
  /// the service.
  #[prost(int32, optional, tag = "79")]
  pub trusted_pid: ::core::option::Option<i32>,
  /// Incrementally emitted interned data, valid only on the packet's sequence
  /// (packets with the same |trusted_packet_sequence_id|). The writer will
  /// usually emit new interned data in the same TracePacket that first refers to
  /// it (since the last reset of interning state). It may also be emitted
  /// proactively in advance of referring to them in later packets.
  #[prost(message, optional, tag = "12")]
  pub interned_data: ::core::option::Option<InternedData>,
  #[prost(uint32, optional, tag = "13")]
  pub sequence_flags: ::core::option::Option<u32>,
  /// DEPRECATED. Moved to SequenceFlags::SEQ_INCREMENTAL_STATE_CLEARED.
  #[prost(bool, optional, tag = "41")]
  pub incremental_state_cleared: ::core::option::Option<bool>,
  /// Default values for fields of later TracePackets emitted on this packet's
  /// sequence (TracePackets with the same |trusted_packet_sequence_id|).
  /// It must be reemitted when incremental state is cleared (see
  /// |incremental_state_cleared|).
  /// Requires that any future packet emitted on the same sequence specifies
  /// the SEQ_NEEDS_INCREMENTAL_STATE flag.
  /// TracePacketDefaults always override the global defaults for any future
  /// packet on this sequence (regardless of SEQ_NEEDS_INCREMENTAL_STATE).
  #[prost(message, optional, tag = "59")]
  pub trace_packet_defaults: ::core::option::Option<TracePacketDefaults>,
  /// Flag set by the service if, for the current packet sequence (see
  /// |trusted_packet_sequence_id|), either:
  /// * this is the first packet, or
  /// * one or multiple packets were dropped since the last packet that the
  ///    consumer read from the sequence. This can happen if chunks in the trace
  ///    buffer are overridden before the consumer could read them when the trace
  ///    is configured in ring buffer mode.
  ///
  /// When packet loss occurs, incrementally emitted data (including interned
  /// data) on the sequence should be considered invalid up until the next packet
  /// with SEQ_INCREMENTAL_STATE_CLEARED set.
  #[prost(bool, optional, tag = "42")]
  pub previous_packet_dropped: ::core::option::Option<bool>,
  /// Flag set by a producer (starting from SDK v29) if, for the current packet
  /// sequence (see |trusted_packet_sequence_id|), this is the first packet.
  ///
  /// This flag can be used for distinguishing the two situations when
  /// processing the trace:
  /// 1. There are no prior events for the sequence because of data loss, e.g.
  ///     due to ring buffer wrapping.
  /// 2. There are no prior events for the sequence because it didn't start
  ///     before this packet (= there's definitely no preceding data loss).
  ///
  /// Given that older SDK versions do not support this flag, this flag not
  /// being present for a particular sequence does not necessarily imply data
  /// loss.
  #[prost(bool, optional, tag = "87")]
  pub first_packet_on_sequence: ::core::option::Option<bool>,
  /// The machine ID for identifying trace packets in a multi-machine tracing
  /// session. Is emitted by the tracing service for producers running on a
  /// remote host (e.g. a VM guest). For more context: go/crosetto-vm-tracing.
  #[prost(uint32, optional, tag = "98")]
  pub machine_id: ::core::option::Option<u32>,
  #[prost(
    oneof = "trace_packet::Data",
    tags = "2, 9, 4, 5, 6, 7, 11, 89, 33, 34, 35, 37, 74, 75, 38, 40, 39, 45, 46, 109, 47, 48, 49, 51, 52, 53, 54, 56, 57, 62, 63, 65, 66, 67, 68, 69, 70, 71, 73, 76, 77, 78, 80, 81, 82, 83, 84, 86, 91, 61, 64, 60, 43, 44, 1, 36, 50, 72, 88, 92, 90, 93, 94, 96, 97, 104, 105, 112, 95, 99, 100, 101, 102, 103, 107, 110, 111, 113, 114, 115, 116, 117, 118, 120, 122, 119, 121, 123, 900"
  )]
  pub data: ::core::option::Option<trace_packet::Data>,
  /// Trusted user id of the producer which generated this packet. Keep in sync
  /// with TrustedPacket.trusted_uid.
  ///
  /// TODO(eseckler): Emit this field in a PacketSequenceDescriptor message
  /// instead.
  #[prost(oneof = "trace_packet::OptionalTrustedUid", tags = "3")]
  pub optional_trusted_uid: ::core::option::Option<trace_packet::OptionalTrustedUid>,
  /// Service-assigned identifier of the packet sequence this packet belongs to.
  /// Uniquely identifies a producer + writer pair within the tracing session. A
  /// value of zero denotes an invalid ID. Keep in sync with
  /// TrustedPacket.trusted_packet_sequence_id.
  #[prost(oneof = "trace_packet::OptionalTrustedPacketSequenceId", tags = "10")]
  pub optional_trusted_packet_sequence_id:
    ::core::option::Option<trace_packet::OptionalTrustedPacketSequenceId>,
}
/// Nested message and enum types in `TracePacket`.
pub mod trace_packet {
  #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
  #[repr(i32)]
  pub enum SequenceFlags {
    SeqUnspecified = 0,
    /// Set by the writer to indicate that it will re-emit any incremental data
    /// for the packet's sequence before referring to it again. This includes
    /// interned data as well as periodically emitted data like
    /// Process/ThreadDescriptors. This flag only affects the current packet
    /// sequence (see |trusted_packet_sequence_id|).
    ///
    /// When set, this TracePacket and subsequent TracePackets on the same
    /// sequence will not refer to any incremental data emitted before this
    /// TracePacket. For example, previously emitted interned data will be
    /// re-emitted if it is referred to again.
    ///
    /// When the reader detects packet loss (|previous_packet_dropped|), it needs
    /// to skip packets in the sequence until the next one with this flag set, to
    /// ensure intact incremental data.
    SeqIncrementalStateCleared = 1,
    /// This packet requires incremental state, such as TracePacketDefaults or
    /// InternedData, to be parsed correctly. The trace reader should skip this
    /// packet if incremental state is not valid on this sequence, i.e. if no
    /// packet with the SEQ_INCREMENTAL_STATE_CLEARED flag has been seen on the
    /// current |trusted_packet_sequence_id|.
    SeqNeedsIncrementalState = 2,
  }
  impl SequenceFlags {
    /// String value of the enum field names used in the ProtoBuf definition.
    ///
    /// The values are not transformed in any way and thus are considered stable
    /// (if the ProtoBuf definition does not change) and safe for programmatic use.
    pub fn as_str_name(&self) -> &'static str {
      match self {
        Self::SeqUnspecified => "SEQ_UNSPECIFIED",
        Self::SeqIncrementalStateCleared => "SEQ_INCREMENTAL_STATE_CLEARED",
        Self::SeqNeedsIncrementalState => "SEQ_NEEDS_INCREMENTAL_STATE",
      }
    }
    /// Creates an enum from field names used in the ProtoBuf definition.
    pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
      match value {
        "SEQ_UNSPECIFIED" => Some(Self::SeqUnspecified),
        "SEQ_INCREMENTAL_STATE_CLEARED" => Some(Self::SeqIncrementalStateCleared),
        "SEQ_NEEDS_INCREMENTAL_STATE" => Some(Self::SeqNeedsIncrementalState),
        _ => None,
      }
    }
  }
  #[derive(Clone, PartialEq, ::prost::Oneof)]
  pub enum Data {
    #[prost(message, tag = "6")]
    ClockSnapshot(super::ClockSnapshot),
    #[prost(message, tag = "11")]
    TrackEvent(super::TrackEvent),
    #[prost(message, tag = "89")]
    TraceUuid(super::TraceUuid),
    /// Only used by TrackEvent.
    #[prost(message, tag = "60")]
    TrackDescriptor(super::TrackDescriptor),
    /// Deprecated, use TrackDescriptor instead.
    #[prost(message, tag = "43")]
    ProcessDescriptor(super::ProcessDescriptor),
    /// Deprecated, use TrackDescriptor instead.
    #[prost(message, tag = "44")]
    ThreadDescriptor(super::ThreadDescriptor),
    /// This field is emitted at periodic intervals (~10s) and
    /// contains always the binary representation of the UUID
    /// {82477a76-b28d-42ba-81dc-33326d57a079}. This is used to be able to
    /// efficiently partition long traces without having to fully parse them.
    #[prost(bytes, tag = "36")]
    SynchronizationMarker(::prost::alloc::vec::Vec<u8>),
    /// Zero or more proto encoded trace packets compressed using deflate.
    /// Each compressed_packets TracePacket (including the two field ids and
    /// sizes) should be less than 512KB.
    #[prost(bytes, tag = "50")]
    CompressedPackets(::prost::alloc::vec::Vec<u8>),
    /// The "range of interest" for track events. See the message definition
    /// comments for more details.
    #[prost(message, tag = "90")]
    TrackEventRangeOfInterest(super::TrackEventRangeOfInterest),
  }
  /// Trusted user id of the producer which generated this packet. Keep in sync
  /// with TrustedPacket.trusted_uid.
  ///
  /// TODO(eseckler): Emit this field in a PacketSequenceDescriptor message
  /// instead.
  #[derive(Clone, Copy, PartialEq, Eq, Hash, ::prost::Oneof)]
  pub enum OptionalTrustedUid {
    #[prost(int32, tag = "3")]
    TrustedUid(i32),
  }
  /// Service-assigned identifier of the packet sequence this packet belongs to.
  /// Uniquely identifies a producer + writer pair within the tracing session. A
  /// value of zero denotes an invalid ID. Keep in sync with
  /// TrustedPacket.trusted_packet_sequence_id.
  #[derive(Clone, Copy, PartialEq, Eq, Hash, ::prost::Oneof)]
  pub enum OptionalTrustedPacketSequenceId {
    #[prost(uint32, tag = "10")]
    TrustedPacketSequenceId(u32),
  }
}
