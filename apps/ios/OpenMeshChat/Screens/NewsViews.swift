import SwiftUI
import HoplaKit

/// News personalisation (§5.115). Stays on this phone (UserDefaults).
final class NewsPrefs: ObservableObject {
    private let d = UserDefaults.standard
    @Published var muted: Set<String> { didSet { d.set(muted.sorted(), forKey: "omcp.news.muted") } }
    @Published var topics: Set<String> { didSet { d.set(topics.sorted(), forKey: "omcp.news.topics") } }
    @Published var city: String { didSet { d.set(city, forKey: "omcp.news.city") } }
    /// §5.111: on by default; the user can turn it off.
    @Published var emergencyAlerts: Bool { didSet { d.set(emergencyAlerts, forKey: "omcp.news.alerts") } }

    init() {
        muted = Set(d.stringArray(forKey: "omcp.news.muted") ?? [])
        topics = Set(d.stringArray(forKey: "omcp.news.topics") ?? [])
        city = d.string(forKey: "omcp.news.city") ?? ""
        emergencyAlerts = d.object(forKey: "omcp.news.alerts") as? Bool ?? true
    }

    func mute(_ t: String) { if t != "emergencies" { muted.insert(t) } }
    func unmute(_ t: String) { muted.remove(t) }
    func toggleTopic(_ t: String) { if topics.contains(t) { topics.remove(t) } else { topics.insert(t) } }
}

/// §5.108: pinned emergency alerts first, muted topics hidden (alerts never hidden).
func arrangeNews(_ stories: [NewsStory], muted: Set<String>) -> [NewsStory] {
    let visible = stories.filter { $0.emergency || !muted.contains($0.topic.lowercased()) }
    return visible.filter { $0.emergency } + visible.filter { !$0.emergency }
}

/// Text-only source pill (§5.109): surface fill, 1 pt ink-2 border, no logo or brand colour.
struct SourceLabel: View {
    @Environment(\.omcpColors) private var c
    let name: String
    var body: some View {
        Text(name).font(.caption2.weight(.semibold)).foregroundStyle(c.ink)
            .padding(.horizontal, 8).padding(.vertical, 2)
            .background(Capsule().fill(c.surface))
            .overlay(Capsule().stroke(c.ink2, lineWidth: 1))
            .accessibilityElement(children: .ignore)
            .accessibilityLabel(L("news_source_cd", name))
    }
}

/// Compact topic chip (`.nchip`): 32 pt visual, ≥ 44 pt hit area (WCAG 2.5.8).
struct TopicChip: View {
    @Environment(\.omcpColors) private var c
    let label: String
    let selected: Bool
    let action: () -> Void
    var body: some View {
        Button(action: action) {
            Text(label).font(.footnote.weight(.semibold))
                .foregroundStyle(selected ? c.brandInk : c.ink2)
                .padding(.horizontal, 12).frame(minHeight: 32)
                .background(Capsule().fill(selected ? c.brand : c.surface))
                .overlay(Capsule().stroke(selected ? c.brand : c.line, lineWidth: 1))
                .omcpHitTarget()
        }
        .buttonStyle(.plain)
        .accessibilityAddTraits(selected ? [.isSelected] : [])
    }
}

private func newsTabLabel(_ t: String) -> String { L("news_tab_" + t) }

struct StoryRow: View {
    @Environment(\.omcpColors) private var c
    let s: NewsStory

    private var spoken: String {
        var parts: [String] = []
        if s.emergency { parts.append(L("news_emergency") + ", " + L("news_pinned")) }
        parts.append(s.title)
        parts.append(L("news_source_cd", s.sourceName))
        if let v = s.via { parts.append(L("news_via", v)) }
        return parts.joined(separator: ", ")
    }
    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            if s.emergency {
                HStack(spacing: 4) {
                    Image(systemName: "exclamationmark.triangle.fill").foregroundStyle(c.danger)
                    Text(L("news_emergency").uppercased()).font(.caption.bold()).foregroundStyle(c.danger)
                    Spacer()
                    Image(systemName: "pin.fill").foregroundStyle(c.ink2)
                    Text(L("news_pinned")).font(.caption).foregroundStyle(c.ink2)
                }
            } else {
                Text(newsTabLabel(s.topic.lowercased())).font(.caption).foregroundStyle(c.ink3)
            }
            Text(s.title).font(.headline).foregroundStyle(c.ink)
            HStack(spacing: 8) {
                SourceLabel(name: s.sourceName)
                if let v = s.via { Text(L("news_via", v)).font(.caption).foregroundStyle(c.ink2) }
            }
            if s.emergency { Text(L("news_alert_from")).font(.caption).foregroundStyle(c.ink2) }
        }
        .padding(12)
        .background(RoundedRectangle(cornerRadius: 14).fill(c.surface))
        .overlay(RoundedRectangle(cornerRadius: 14).stroke(s.emergency ? c.danger : c.line, lineWidth: s.emergency ? 2 : 1))
        // One focus stop: "headline, source, via contact" (§5.109).
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(spoken)
        .accessibilityAddTraits(.isButton)
    }
}

/// News tab (§5.108): settings, chips, pinned alerts, newest first. No publish entry anywhere.
struct NewsHomeView: View {
    @EnvironmentObject var model: AppModel
    @Environment(\.omcpColors) private var c
    @StateObject private var prefs = NewsPrefs()
    @StateObject private var net = NetworkWatcher()
    @State private var tab = "top"
    @State private var stories: [NewsStory] = []
    @State private var meteredWarning = false

    var body: some View {
        NavigationStack {
            VStack(spacing: 0) {
                ScrollView(.horizontal, showsIndicators: false) {
                    HStack(spacing: 6) {
                        ForEach(newsTabs, id: \.self) { t in
                            TopicChip(label: newsTabLabel(t), selected: tab == t) { tab = t; reload() }
                                .contextMenu {
                                    // Shortcut only; News settings has a single-tap "Mute a topic" (WCAG 2.5.7).
                                    if t != "emergencies" { Button(L("news_mute_topic")) { prefs.mute(t); reload() } }
                                }
                        }
                    }.padding(.horizontal, 12)
                }
                .background(c.surface)
                Divider().overlay(c.line)
                if meteredWarning {
                    Text(L("news_metered_warning")).font(.caption).foregroundStyle(c.danger).padding(.horizontal, 16).padding(.top, 8)
                }
                if tab == "world" || tab == "top" {
                    VStack(alignment: .leading, spacing: 2) {
                        if tab == "world" { Text(L("news_whl")).font(.subheadline.bold()).accessibilityAddTraits(.isHeader) }
                        Text(L("news_whl_d")).font(.caption).foregroundStyle(c.ink2)
                    }.frame(maxWidth: .infinity, alignment: .leading).padding(.horizontal, 16).padding(.vertical, 8)
                }
                if stories.isEmpty {
                    VStack(spacing: 16) {
                        EmptyStateView(title: L("news_empty_title"), bodyText: L("news_empty_body"))
                        NavigationLink(L("news_empty_btn")) { NewsSettingsView(prefs: prefs) }
                            .buttonStyle(.borderedProminent).omcpHitTarget()
                    }
                } else {
                    ScrollView {
                        LazyVStack(spacing: 12) {
                            ForEach(stories) { s in
                                NavigationLink { NewsReaderView(story: s, prefs: prefs) } label: { StoryRow(s: s) }
                                    .buttonStyle(.plain)
                            }
                        }.padding(16)
                    }
                }
            }
            .background(c.bg)
            .navigationTitle(L("tab_news"))
            .toolbar {
                ToolbarItem(placement: .topBarTrailing) {
                    NavigationLink { NewsSettingsView(prefs: prefs) } label: {
                        Image(systemName: "slider.horizontal.3").omcpHitTarget()
                    }
                    .accessibilityLabel(L("news_settings"))
                }
            }
            .onAppear(perform: reload)
            // Any phone with internet fetches Google News RSS under the network rules (Wi-Fi-only by default).
            .task {
                while !Task.isCancelled {
                    await fetchRound()
                    try? await Task.sleep(nanoseconds: 300 * 1_000_000_000)
                }
            }
        }
    }

    private func reload() { stories = arrangeNews(model.core.newsFeed(tab: tab), muted: prefs.muted) }

    @MainActor
    private func fetchRound() async {
        let now = Int64(Date().timeIntervalSince1970 * 1000)
        guard let plan = NewsFetchRules.plan(core: model.core, store: model.store, network: net.type, metered: net.metered, nowMs: now) else { return }
        meteredWarning = plan.meteredWarning
        for feed in plan.feeds {
            guard let url = URL(string: feed), url.scheme == "https",
                  let (data, _) = try? await URLSession.shared.data(from: url) else { continue }
            let body = String(decoding: data.prefix(1_000_000), as: UTF8.self)
            _ = try? model.core.newsIngestRss(feedUrl: feed, body: body, nowMs: now)
        }
        if !plan.feeds.isEmpty { reload() }
    }
}

/// Reader (§5.110): summary + "Read on Google News"; Share (primary) and More in the bottom bar.
struct NewsReaderView: View {
    @EnvironmentObject var model: AppModel
    @Environment(\.omcpColors) private var c
    @Environment(\.openURL) private var openURL
    @Environment(\.dismiss) private var dismiss
    let story: NewsStory
    @ObservedObject var prefs: NewsPrefs
    @State private var sharing = false

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 12) {
                Text(newsTabLabel(story.topic.lowercased())).font(.caption).foregroundStyle(c.ink3)
                Text(story.title).font(.title2.bold()).accessibilityAddTraits(.isHeader)
                HStack(spacing: 8) {
                    SourceLabel(name: story.sourceName)
                    if let v = story.via { Text(L("news_via", v)).font(.caption).foregroundStyle(c.ink2) }
                }
                if story.via != nil { Text(L("news_fetched_by")).font(.caption).foregroundStyle(c.ink2) }
                if !story.summary.isEmpty {
                    Text(L("news_summary")).font(.headline).accessibilityAddTraits(.isHeader)
                    Text(story.summary)
                }
                Button(L("news_read_gn")) { if let u = URL(string: story.link) { openURL(u) } }
                    .buttonStyle(.bordered).omcpHitTarget()
                Text(L("news_read_gn_note")).font(.caption).foregroundStyle(c.ink2)
            }.padding()
        }
        .navigationTitle(story.sourceName)
        .safeAreaInset(edge: .bottom) {
            HStack {
                Button(L("news_share")) { sharing = true }
                    .buttonStyle(.borderedProminent).frame(maxWidth: .infinity, minHeight: 44)
                Menu {
                    Button(L("news_mute_this_topic")) { prefs.mute(story.topic.lowercased()); dismiss() }
                    Button(L("news_copy_link")) { UIPasteboard.general.string = story.link }
                } label: { Image(systemName: "ellipsis.circle").omcpHitTarget() }
                .accessibilityLabel(L("news_more"))
            }
            .padding(.horizontal).padding(.vertical, 8)
            .background(c.surface)
        }
        .sheet(isPresented: $sharing) { NewsShareSheet(story: story) }
    }
}

/// Share sheet (§5.113): preview, Chats (single select), Communities (v0.2), note, Send; Close always visible.
struct NewsShareSheet: View {
    @EnvironmentObject var model: AppModel
    @Environment(\.omcpColors) private var c
    @Environment(\.dismiss) private var dismiss
    let story: NewsStory
    @State private var pick: String?

    var body: some View {
        NavigationStack {
            List {
                Section {
                    VStack(alignment: .leading, spacing: 6) {
                        Text(story.title).font(.subheadline.bold())
                        SourceLabel(name: story.sourceName)
                    }
                }
                Section(L("news_chats_h")) {
                    ForEach(model.repo.contacts) { ct in
                        Button { pick = ct.id } label: {
                            HStack {
                                Text(ct.displayName).foregroundStyle(c.ink)
                                Spacer()
                                if pick == ct.id { Image(systemName: "checkmark").foregroundStyle(c.brand) }
                            }.frame(minHeight: 44)
                        }
                        .accessibilityAddTraits(pick == ct.id ? [.isSelected] : [])
                    }
                }
                Section(L("news_communities_h")) { Text(L("coming_v02_title")).foregroundStyle(c.ink2) }
                Section(footer: Text(L("news_share_note"))) {
                    Button(L("news_send")) {
                        if let id = pick, let text = model.core.newsCardText(id: story.id) {
                            try? model.repo.send(id, text: text)
                            model.touch()
                        }
                        dismiss()
                    }
                    .disabled(pick == nil)
                    .frame(minHeight: 44)
                }
            }
            .navigationTitle(L("news_sh_title"))
            .toolbar { ToolbarItem(placement: .cancellationAction) { Button(L("close")) { dismiss() } } }
        }
    }
}

/// News settings (§5.115): topics, city, muted topics with a single-tap "Mute a topic"
/// (WCAG 2.5.7), emergency alerts toggle (§5.111), storage note.
struct NewsSettingsView: View {
    @EnvironmentObject var model: AppModel
    @Environment(\.omcpColors) private var c
    @ObservedObject var prefs: NewsPrefs

    var body: some View {
        Form {
            Section {
                FlowChips(items: newsTabs.filter { $0 != "top" }, selected: prefs.topics, label: newsTabLabel) { prefs.toggleTopic($0) }
            } header: { Text(L("news_topics")) } footer: { Text(L("news_int_d")) }
            Section {
                TextField(L("news_or_type"), text: $prefs.city).textContentType(.addressCity)
            } header: { Text(L("news_city")) } footer: { Text(L("news_loc_note")) }
            Section(L("news_mute_topics")) {
                if prefs.muted.isEmpty { Text(L("news_mute_none")).foregroundStyle(c.ink2) }
                ForEach(prefs.muted.sorted(), id: \.self) { t in
                    HStack {
                        Text(newsTabLabel(t) + " · " + L("news_muted_t"))
                        Spacer()
                        Button(L("news_unmute")) { prefs.unmute(t) }.omcpHitTarget()
                    }
                }
                Menu(L("news_mute_topic")) {
                    ForEach(newsTabs.filter { $0 != "emergencies" && !prefs.muted.contains($0) }, id: \.self) { t in
                        Button(newsTabLabel(t)) { prefs.mute(t) }
                    }
                }.frame(minHeight: 44)
            }
            Section {
                Toggle(L("news_alert_notif"), isOn: $prefs.emergencyAlerts)
            } header: { Text(L("news_alert_title")) } footer: { Text(L("news_alert_notif_d")) }
            Section {
                Text(L("news_storage_d")).foregroundStyle(c.ink2)
            } header: { Text(L("news_storage")) }
        }
        .navigationTitle(L("news_settings"))
        .onDisappear {
            model.core.newsSetPrefs(topics: prefs.topics.sorted(), langs: ["en"], city: prefs.city, muted: prefs.muted.sorted(), alertsNotify: prefs.emergencyAlerts)
        }
    }
}

/// Wrapping row of selectable chips (simple, no custom Layout needed).
struct FlowChips: View {
    let items: [String]
    let selected: Set<String>
    let label: (String) -> String
    let toggle: (String) -> Void
    var body: some View {
        LazyVGrid(columns: [GridItem(.adaptive(minimum: 110), spacing: 6)], alignment: .leading, spacing: 6) {
            ForEach(items, id: \.self) { t in
                TopicChip(label: label(t), selected: selected.contains(t)) { toggle(t) }
            }
        }
    }
}
