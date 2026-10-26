import SwiftUI
import HoplaKit

/// News tab (design §5.108): chips, pinned emergency alerts, newest first. No publish entry anywhere.
struct NewsHomeView: View {
    @EnvironmentObject var model: AppModel
    @State private var tab = "top"
    @State private var stories: [NewsStory] = []

    var body: some View {
        NavigationStack {
            VStack(spacing: 0) {
                ScrollView(.horizontal, showsIndicators: false) {
                    HStack {
                        ForEach(newsTabs, id: \.self) { t in
                            Button(L("news_tab_" + t)) { tab = t; reload() }
                                .buttonStyle(.bordered)
                                .tint(tab == t ? .accentColor : .secondary)
                        }
                    }.padding(.horizontal)
                }
                if stories.isEmpty {
                    EmptyStateView(title: L("news_empty_title"), bodyText: L("news_empty_body"))
                } else {
                    List(stories) { s in
                        NavigationLink { NewsReaderView(story: s) } label: {
                            VStack(alignment: .leading, spacing: 4) {
                                if s.emergency { Text(L("news_emergency_pinned")).font(.caption).foregroundStyle(.red) }
                                Text(s.title).font(.headline)
                                HStack {
                                    Text(s.sourceName).font(.caption2).padding(.horizontal, 6)
                                        .overlay(RoundedRectangle(cornerRadius: 4).stroke(.secondary))
                                        .accessibilityLabel(L("news_source_cd", s.sourceName))
                                    Text(s.publisher + (s.via.map { " · " + L("news_via", $0) } ?? "")).font(.caption)
                                }
                            }
                        }
                    }
                }
            }
            .navigationTitle(L("tab_news"))
            .onAppear(perform: reload)
        }
    }

    private func reload() { stories = model.core.newsFeed(tab: tab) }
}

/// Reader (design §5.110): summary + "Read on Google News".
struct NewsReaderView: View {
    let story: NewsStory
    @Environment(\.openURL) private var openURL

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 12) {
                Text(story.title).font(.title2.bold())
                Text(story.publisher + (story.via.map { " · " + L("news_via", $0) } ?? "")).font(.caption)
                if !story.summary.isEmpty {
                    Text(L("news_summary")).font(.headline)
                    Text(story.summary)
                }
                Button(L("news_read_gn")) { if let u = URL(string: story.link) { openURL(u) } }
                    .buttonStyle(.bordered)
                Text(L("news_read_gn_note")).font(.caption).foregroundStyle(.secondary)
            }.padding()
        }
        .navigationTitle(story.sourceName)
    }
}
