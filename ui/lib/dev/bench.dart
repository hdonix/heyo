// Measuring how long frames take while something runs, for development:
// `--bench=…` (or `HEYO_UI_BENCH=…` in Heyo's environment) has a view drive
// itself, and the result is printed as one line starting "BENCH ", on
// stderr (which joins Heyo's log).

import 'dart:convert';
import 'dart:io';
import 'dart:ui' show FramePhase, FrameTiming;

import 'package:flutter/scheduler.dart';

/// Collects how long frames took, as Flutter reports them (`FrameTiming`),
/// then says so.
class Frames {
  final _timings = <FrameTiming>[];
  late final TimingsCallback _listener = _timings.addAll;

  void start() => SchedulerBinding.instance.addTimingsCallback(_listener);

  /// The summary: medians and 95th percentiles, in milliseconds.
  Future<Map<String, Object>> finish(String what) async {
    // Timings are delivered in batches, up to a second late.
    await Future<void>.delayed(const Duration(milliseconds: 1200));
    SchedulerBinding.instance.removeTimingsCallback(_listener);
    double ms(Duration d) => d.inMicroseconds / 1000;
    List<double> sorted(Iterable<double> values) => values.toList()..sort();
    double at(List<double> values, double q) => values.isEmpty ? 0 : values[(values.length * q).floor().clamp(0, values.length - 1)];
    final build = sorted(_timings.map((t) => ms(t.buildDuration)));
    final raster = sorted(_timings.map((t) => ms(t.rasterDuration)));
    final total = sorted(_timings.map((t) => ms(t.totalSpan)));
    final first = _timings.isEmpty ? 0 : _timings.first.timestampInMicroseconds(FramePhase.vsyncStart);
    final last = _timings.isEmpty ? 0 : _timings.last.timestampInMicroseconds(FramePhase.rasterFinish);
    return {
      'what': what,
      'frames': _timings.length,
      'seconds': ((last - first) / 1e6).toStringAsFixed(2),
      'buildMedianMs': at(build, 0.5).toStringAsFixed(2),
      'rasterMedianMs': at(raster, 0.5).toStringAsFixed(2),
      'rasterP95Ms': at(raster, 0.95).toStringAsFixed(2),
      'totalMedianMs': at(total, 0.5).toStringAsFixed(2),
      'totalP95Ms': at(total, 0.95).toStringAsFixed(2),
      'totalWorstMs': at(total, 1).toStringAsFixed(2),
      'over16ms': total.where((t) => t > 16.7).length,
    };
  }

  /// Prints the summary.
  Future<void> report(String what) async => stderr.writeln('BENCH ${jsonEncode(await finish(what))}');
}
