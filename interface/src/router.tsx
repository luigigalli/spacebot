import {
	createRouter,
	createRootRoute,
	createRoute,
	Outlet,
} from "@tanstack/react-router";
import {BASE_PATH} from "@/api/client";
import {ConnectionBanner} from "@/components/ConnectionBanner";
import {TopBar, TopBarProvider, useSetTopBar} from "@/components/TopBar";
import {Sidebar} from "@/components/Sidebar";
import {Overview} from "@/routes/Overview";
import {AgentDetail} from "@/routes/AgentDetail";
import {AgentChannels} from "@/routes/AgentChannels";
import {AgentCortex} from "@/routes/AgentCortex";
import {ChannelDetail} from "@/routes/ChannelDetail";
import {AgentMemories} from "@/routes/AgentMemories";
import {AgentConfig} from "@/routes/AgentConfig";
import {AgentCron} from "@/routes/AgentCron";
import {AgentIngest} from "@/routes/AgentIngest";
import {AgentSkills} from "@/routes/AgentSkills";
import {AgentWorkers} from "@/routes/AgentWorkers";
import {AgentProjects} from "@/routes/AgentProjects";
import {AgentTasks} from "@/routes/AgentTasks";
import {GlobalTasks} from "@/routes/GlobalTasks";
import {AgentChat} from "@/routes/AgentChat";
import {Settings} from "@/routes/Settings";
import {Orchestrate} from "@/routes/Orchestrate";
import {useLiveContext} from "@/hooks/useLiveContext";
import {FolderOpen} from "@phosphor-icons/react";

// ── Root layout ──────────────────────────────────────────────────────────

function RootLayout() {
	const {liveStates, connectionState, hasData} = useLiveContext();

	return (
		<TopBarProvider>
			<div className="flex h-screen flex-col overflow-hidden bg-app">
				<TopBar />
				<ConnectionBanner state={connectionState} hasData={hasData} />
				<div className="flex min-h-0 flex-1">
					<Sidebar liveStates={liveStates} />
					<div className="flex min-w-0 flex-1 flex-col overflow-hidden">
						<Outlet />
					</div>
				</div>
			</div>
		</TopBarProvider>
	);
}

// ── Routes ───────────────────────────────────────────────────────────────

const rootRoute = createRootRoute({
	component: RootLayout,
});

const indexRoute = createRoute({
	getParentRoute: () => rootRoute,
	path: "/",
	component: function IndexPage() {
		const {liveStates, activeLinks} = useLiveContext();
		return <Overview liveStates={liveStates} activeLinks={activeLinks} />;
	},
});

const dashboardRoute = createRoute({
	getParentRoute: () => rootRoute,
	path: "/dashboard",
	component: function DashboardPage() {
		useSetTopBar(
			<div className="flex h-full items-center px-6">
				<h1 className="font-plex text-sm font-medium text-ink">Dashboard</h1>
			</div>,
		);
		return (
			<div className="flex flex-1 items-center justify-center">
				<p className="text-sm text-ink-faint">Dashboard coming soon</p>
			</div>
		);
	},
});

const settingsRoute = createRoute({
	getParentRoute: () => rootRoute,
	path: "/settings",
	validateSearch: (search: Record<string, unknown>): {tab?: string} => {
		return {
			tab: typeof search.tab === "string" ? search.tab : undefined,
		};
	},
	component: function SettingsPage() {
		return <Settings />;
	},
});

const logsRoute = createRoute({
	getParentRoute: () => rootRoute,
	path: "/logs",
	component: function LogsPage() {
		useSetTopBar(
			<div className="flex h-full items-center px-6">
				<h1 className="font-plex text-sm font-medium text-ink">Logs</h1>
			</div>,
		);
		return (
			<div className="flex flex-1 items-center justify-center">
				<p className="text-sm text-ink-faint">Logs coming soon</p>
			</div>
		);
	},
});

const orchestrateRoute = createRoute({
	getParentRoute: () => rootRoute,
	path: "/orchestrate",
	component: function OrchestratePage() {
		useSetTopBar(
			<div className="flex h-full items-center gap-4 px-6">
				<h1 className="font-plex text-sm font-medium text-ink">
					Orchestrate
				</h1>
				<span className="text-xs text-ink-faint">Active workers across all agents</span>
			</div>,
		);
		return <Orchestrate />;
	},
});

const tasksRoute = createRoute({
	getParentRoute: () => rootRoute,
	path: "/tasks",
	component: function TasksPage() {
		useSetTopBar(
			<div className="flex h-full items-center gap-4 px-6">
				<h1 className="font-plex text-sm font-medium text-ink">Tasks</h1>
				<span className="text-xs text-ink-faint">All tasks across agents</span>
			</div>,
		);
		return <GlobalTasks />;
	},
});

const agentRoute = createRoute({
	getParentRoute: () => rootRoute,
	path: "/agents/$agentId",
	component: function AgentPage() {
		const {agentId} = agentRoute.useParams();
		const {liveStates} = useLiveContext();
		return <AgentDetail agentId={agentId} liveStates={liveStates} />;
	},
});

const agentChatRoute = createRoute({
	getParentRoute: () => rootRoute,
	path: "/agents/$agentId/chat",
	component: function AgentChatPage() {
		const {agentId} = agentChatRoute.useParams();
		return <AgentChat agentId={agentId} />;
	},
});

const agentChannelsRoute = createRoute({
	getParentRoute: () => rootRoute,
	path: "/agents/$agentId/channels",
	component: function AgentChannelsPage() {
		const {agentId} = agentChannelsRoute.useParams();
		const {liveStates} = useLiveContext();
		return <AgentChannels agentId={agentId} liveStates={liveStates} />;
	},
});

const agentMemoriesRoute = createRoute({
	getParentRoute: () => rootRoute,
	path: "/agents/$agentId/memories",
	component: function AgentMemoriesPage() {
		const {agentId} = agentMemoriesRoute.useParams();
		return <AgentMemories agentId={agentId} />;
	},
});

const agentIngestRoute = createRoute({
	getParentRoute: () => rootRoute,
	path: "/agents/$agentId/ingest",
	component: function AgentIngestPage() {
		const {agentId} = agentIngestRoute.useParams();
		return <AgentIngest agentId={agentId} />;
	},
});

const agentWorkersRoute = createRoute({
	getParentRoute: () => rootRoute,
	path: "/agents/$agentId/workers",
	validateSearch: (search: Record<string, unknown>): {worker?: string} => ({
		worker: typeof search.worker === "string" ? search.worker : undefined,
	}),
	component: function AgentWorkersPage() {
		const {agentId} = agentWorkersRoute.useParams();
		return <AgentWorkers agentId={agentId} />;
	},
});

const projectsRoute = createRoute({
	getParentRoute: () => rootRoute,
	path: "/projects",
	component: function ProjectsPage() {
		useSetTopBar(
			<div className="flex h-full items-center gap-4 px-6">
				<FolderOpen className="size-4 text-ink-dull" weight="bold" />
				<h1 className="font-plex text-sm font-medium text-ink">Projects</h1>
				<span className="text-xs text-ink-faint">All projects across agents</span>
			</div>,
		);
		return <AgentProjects />;
	},
});

const agentTasksRoute = createRoute({
	getParentRoute: () => rootRoute,
	path: "/agents/$agentId/tasks",
	component: function AgentTasksPage() {
		const {agentId} = agentTasksRoute.useParams();
		return <AgentTasks agentId={agentId} />;
	},
});

const agentCronRoute = createRoute({
	getParentRoute: () => rootRoute,
	path: "/agents/$agentId/cron",
	component: function AgentCronPage() {
		const {agentId} = agentCronRoute.useParams();
		return <AgentCron agentId={agentId} />;
	},
});

const agentConfigRoute = createRoute({
	getParentRoute: () => rootRoute,
	path: "/agents/$agentId/config",
	validateSearch: (search: Record<string, unknown>): {tab?: string} => {
		return {
			tab: typeof search.tab === "string" ? search.tab : undefined,
		};
	},
	component: function AgentConfigPage() {
		const {agentId} = agentConfigRoute.useParams();
		return <AgentConfig agentId={agentId} />;
	},
});

const agentCortexRoute = createRoute({
	getParentRoute: () => rootRoute,
	path: "/agents/$agentId/cortex",
	component: function AgentCortexPage() {
		const {agentId} = agentCortexRoute.useParams();
		return <AgentCortex agentId={agentId} />;
	},
});

const agentSkillsRoute = createRoute({
	getParentRoute: () => rootRoute,
	path: "/agents/$agentId/skills",
	component: function AgentSkillsPage() {
		const {agentId} = agentSkillsRoute.useParams();
		return <AgentSkills agentId={agentId} />;
	},
});

const channelRoute = createRoute({
	getParentRoute: () => rootRoute,
	path: "/agents/$agentId/channels/$channelId",
	component: function ChannelPage() {
		const {agentId, channelId} = channelRoute.useParams();
		const {liveStates, channels, loadOlderMessages} = useLiveContext();
		const channel = channels.find((c) => c.id === channelId);
		return (
			<ChannelDetail
				agentId={agentId}
				channelId={channelId}
				channel={channel}
				liveState={liveStates[channelId]}
				onLoadMore={() => loadOlderMessages(channelId)}
			/>
		);
	},
});

const routeTree = rootRoute.addChildren([
	indexRoute,
	dashboardRoute,
	settingsRoute,
	logsRoute,
	orchestrateRoute,
	tasksRoute,
	agentRoute,
	agentChatRoute,
	agentChannelsRoute,
	agentMemoriesRoute,
	agentIngestRoute,
	agentWorkersRoute,
	projectsRoute,
	agentTasksRoute,
	agentCortexRoute,
	agentSkillsRoute,
	agentCronRoute,
	agentConfigRoute,
	channelRoute,
]);

export const router = createRouter({
	routeTree,
	basepath: BASE_PATH || "/",
});

declare module "@tanstack/react-router" {
	interface Register {
		router: typeof router;
	}
}
