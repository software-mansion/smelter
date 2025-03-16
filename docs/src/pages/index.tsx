import Link from '@docusaurus/Link';
import Heading from '@theme/Heading';
import Layout from '@theme/Layout';
import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';
import clsx from 'clsx';
import { FaServer } from 'react-icons/fa';
import { FaBook, FaCode, FaDocker, FaGears, FaGithub, FaLink } from 'react-icons/fa6';
import { GiBattery100, GiFeather, GiSpeedometer } from 'react-icons/gi';
import { IoCloudOffline } from 'react-icons/io5';
import { MdAudiotrack, MdLiveTv } from 'react-icons/md';

import { useColorMode } from '@docusaurus/theme-common';
import BroadcastingImg from '@site/static/img/broadcasting.webp';
import ComposingImg from '@site/static/img/how_it_works.webp';
import MembraneLogo from '@site/static/img/membrane-logo.svg';
import StreamingImg from '@site/static/img/streaming.webp';
import SwmLogo from '@site/static/img/swm-logo.svg';
import VideoConferencingImg from '@site/static/img/videoconferencing.webp';
import WebGpuLogoDark from '@site/static/img/webgpu-dark.svg';
import WebGpuLogoLight from '@site/static/img/webgpu-light.svg';
import { PropsWithChildren } from 'react';
import { IconContext, IconType } from 'react-icons';
import TypewriterComponent from 'typewriter-effect';
import ExampleSceneJson from '../components/ExampleSceneJson';
import ExampleSceneJsx from '../components/ExampleSceneJsx';
import styles from './index.module.css';

function HomepageHeader() {
  return (
    <div className={clsx('container', styles.firstSection)}>
      <div className="row">
        <div className={styles.heroText}>
          <div className="container">
            <Heading as="h1">
              <span className="text--primary">Mix video and audio </span>
              <div className={styles.typewriter}>
                <TypewriterComponent
                  options={{
                    strings: ['in real-time.', 'using code.', 'with low latency.'],
                    autoStart: true,
                    loop: true,
                    deleteSpeed: 50,
                    delay: 80,
                  }}
                />
              </div>
            </Heading>
            <p className={styles.mainSubheading}>
              Media server for real-time, low latency, programmable video and audio mixing.
            </p>
            <div className="row">
              <Link
                className={clsx(
                  'button button--primary button--lg col col-6 margin--sm',
                  styles.hoverPrimary
                )}
                to="/docs/intro">
                <IconContext.Provider value={{ style: { verticalAlign: 'middle' } }}>
                  <FaBook style={{ marginRight: 3 }} />
                  Docs
                </IconContext.Provider>
              </Link>
              <Link
                className={clsx(
                  'button button--secondary button--outline button--lg col col-6 margin--sm',
                  styles.hoverSecondary
                )}
                to="https://github.com/software-mansion/live-compositor">
                <IconContext.Provider value={{ style: { verticalAlign: 'middle' } }}>
                  <FaGithub style={{ marginRight: 5 }} />
                  View on GitHub
                </IconContext.Provider>
              </Link>
            </div>
          </div>
        </div>
        <div className={styles.heroVideo}>
          <div
            style={{
              borderRadius: '15px',
              overflow: 'hidden',
              boxShadow: '0px 0px 20px 1px #4332a6',
            }}>
            <video
              loop
              autoPlay
              muted
              src="/video/showcase.mp4"
              poster="/img/showcase_poster.webp"
              style={{ width: '100%', display: 'block' }}
            />
          </div>
        </div>
      </div>
    </div>
  );
}

function HowItWorks() {
  return (
    <div className="container margin-top--md">
      <Heading as="h2" className="margin-bottom--md text--center text--primary">
        How does it work?
      </Heading>
      <p className={clsx('text--center', styles.grayText)}>
        1. Either stream inputs in or provide them as MP4 files
        <br />
        2. Configure mixing with React components or via HTTP API
        <br />
        3. Write the outputs to a file or stream them to a separate service
      </p>
      <div className="flex flex-row flex-wrap">
        <div className="flex-1 mx-8 mt-20 min-w-[400px]">
          <img src={ComposingImg} alt="Composing" />
        </div>
        <div className="flex-1">
          <Tabs>
            <TabItem value="react" label="React" default>
              <ExampleSceneJsx />
            </TabItem>
            <TabItem value="json" label="JSON">
              <ExampleSceneJson />
            </TabItem>
          </Tabs>
        </div>
      </div>
    </div>
  );
}

function UseCases() {
  return (
    <div className="container margin-top--md">
      <Heading as="h2" className="margin-bottom--md text--center text--primary">
        Use cases
      </Heading>
      <p className={clsx('text--center', styles.grayText)}>
        Use LiveCompositor for video conferencing, live-streaming, broadcasting, and more.
      </p>
      <div className="row">
        <UseCaseCard
          title="Video conferencing"
          subtitle="Stream or record video conferences"
          img={VideoConferencingImg}
        />
        <UseCaseCard title="Broadcasting" subtitle="Compose broadcasts" img={BroadcastingImg} />
        <UseCaseCard
          title="Live-streaming"
          subtitle="Create awesome live-streams"
          img={StreamingImg}
        />
      </div>
    </div>
  );
}

type UseCaseCardProps = {
  title: string;
  subtitle: string;
  img: any;
};

function UseCaseCard(props: UseCaseCardProps) {
  return (
    <div className={clsx('card', styles.card, styles.hoverPrimary)}>
      <div className="text--primary">
        <Heading as="h3" style={{ textAlign: 'center', margin: 0 }}>
          {props.title}
        </Heading>
      </div>
      <p className={clsx('text--center', styles.grayText)} style={{ margin: 0 }}>
        {props.subtitle}
      </p>
      <div className="card__body">
        <img src={props.img} alt={props.title} />
      </div>
    </div>
  );
}

function VisionCards() {
  return (
    <div className="container margin-top--md">
      <Heading as="h2" className="margin-bottom--md text--center text--primary">
        Vision
      </Heading>
      <p className={clsx('text--center', styles.grayText)}>
        Make mixing live streams as simple as building a website.
      </p>
      <div className="row">
        <VisionCard title="Simple declarative API" icon={FaCode}>
          <p className="padding--md">
            Mixing is specified using simple component tree, easy to pick up for anyone familiar
            with web development.
          </p>
        </VisionCard>
        <VisionCard title="Easy integration" icon={FaLink}>
          <p className="padding--md">
            API is language agnostic. You can leverage tech stack of your choice and easily
            integrate it with your own solutions.
          </p>
        </VisionCard>
        <VisionCard title="Real-time and low latency" icon={GiSpeedometer}>
          <p className="padding--md">
            LiveCompositor targets real-time use cases, with a significant focus on situations where
            latency is critical.
          </p>
        </VisionCard>
      </div>
    </div>
  );
}

type VisionCardProps = {
  title: string;
  icon: IconType;
};

function VisionCard(props: PropsWithChildren<VisionCardProps>) {
  const Icon = props.icon;
  return (
    <div className={clsx('card', styles.card, styles.hoverPrimary)}>
      <div style={{ display: 'flex', justifyContent: 'center' }}>
        <Icon className={styles.icon} />
      </div>
      <div className="card__header">
        <Heading as="h3" style={{ textAlign: 'center' }}>
          {props.title}
        </Heading>
      </div>
      <div className="card__body">{props.children}</div>
    </div>
  );
}

type FeatureProps = {
  text: string;
  secondaryText?: string;
  image: any;
};

function Feature(props: PropsWithChildren<FeatureProps>) {
  const text = (
    <div className="col">
      <Heading as="h3" className="margin-top--sm">
        {props.text}
      </Heading>
      <p>{props.secondaryText}</p>
    </div>
  );
  return (
    <div className="row margin-right--lg margin-left--lg" style={{ alignItems: 'center' }}>
      {props.image}
      <div className="col">
        {text}
        {props.children}
      </div>
    </div>
  );
}

function Features() {
  const { colorMode } = useColorMode();

  const wgpuLogo =
    colorMode === 'dark' ? (
      <WebGpuLogoDark className={styles.featureIcon} width={80} height={80} />
    ) : (
      <WebGpuLogoLight className={styles.featureIcon} width={80} height={80} />
    );

  return (
    <div className="container margin-top--lg margin-bottom--md">
      <Heading as="h2" className="margin-bottom--md text--center text--primary">
        Capabilities
      </Heading>
      <p className={clsx('text--center', styles.grayText)}>Simple, powerful, fast. Pick three.</p>

      <Feature
        text="Batteries included"
        secondaryText="Process streams, render text, images, GIFs or websites, and combine them into output streams using high-level components."
        image={<GiBattery100 className={styles.featureIcon} style={{ fontSize: 80 }} />}
      />

      <Feature
        text="Highly customizable"
        secondaryText="Create custom effects with WGSL shaders and directly leverage GPU parallel processing capabilities."
        image={<FaGears className={styles.featureIcon} style={{ fontSize: 80 }} />}
      />

      <Feature
        text="Audio support"
        secondaryText="Mix audio directly in LiveCompositor."
        image={<MdAudiotrack className={styles.featureIcon} style={{ fontSize: 80 }} />}
      />

      <Feature
        text="WebGPU and Rust"
        secondaryText="Leverage Rust and WebGPU rendering for great performance."
        image={wgpuLogo}
      />

      <Feature
        text="Animated transitions"
        secondaryText="Change layouts on the fly with animated transitions."
        image={<GiFeather className={styles.featureIcon} style={{ fontSize: 80 }} />}
      />

      <Feature
        text="Real-time processing"
        secondaryText="Process live streams in real-time with low latency."
        image={<MdLiveTv className={styles.featureIcon} style={{ fontSize: 80 }} />}
      />

      <Feature
        text="Offline processing"
        secondaryText="Use LiveCompositor for non-real-time use cases with offline processing mode."
        image={<IoCloudOffline className={styles.featureIcon} style={{ fontSize: 80 }} />}
      />
    </div>
  );
}

function StandaloneSever() {
  return (
    <div className="row" style={{ justifyContent: 'center' }}>
      <FaServer className="margin--lg" style={{ fontSize: 150, alignSelf: 'center' }} />
      <div className="col">
        <div className="card">
          <div className="card__header">
            <Heading as="h3" className="margin-top--sm">
              Standalone server
            </Heading>
          </div>
          <div className="card__body container">
            <p>
              LiveCompositor can be deployed as a standalone server. Language-agnostic API allows
              you to seamlessly integrate it into your existing solutions. Dockerfiles and
              deployment section in docs are meant to make the deployment process as easy as
              possible.
            </p>
            <div className="row margin--sm" style={{ justifyContent: 'end' }}>
              <Link
                className={clsx(
                  'button button--secondary button--outline button--lg margin--sm',
                  styles.hoverSecondary,
                  styles.smallScreenFlexButton
                )}
                to="/docs/deployment/overview">
                <IconContext.Provider value={{ style: { verticalAlign: 'middle' } }}>
                  <FaBook style={{ marginRight: 5 }} />
                  Docs
                </IconContext.Provider>
              </Link>
              <Link
                className={clsx(
                  'button button--secondary button--outline button--lg margin--sm',
                  styles.hoverSecondary,
                  styles.smallScreenFlexButton
                )}
                to="https://github.com/software-mansion/live-compositor/tree/master/build_tools/docker">
                <IconContext.Provider value={{ style: { verticalAlign: 'middle' } }}>
                  <FaDocker style={{ marginRight: 5 }} />
                  Dockerfiles
                </IconContext.Provider>
              </Link>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}

function MembranePlugin() {
  return (
    <div className="row" style={{ justifyContent: 'center' }}>
      <MembraneLogo
        width={150}
        height={150}
        className="margin--lg"
        style={{ alignSelf: 'center' }}
      />
      <div className="col">
        <div className="card">
          <div className="card__header">
            <Heading as="h3" className="margin-top--sm">
              Membrane plugin
            </Heading>
          </div>
          <div className="card__body container">
            <p>
              Membrane is a developer-friendly multimedia framework for Elixir. You can easily add
              video and audio composing functionality into your multimedia pipeline using Membrane
              LiveCompositor Plugin.
            </p>
            <div className="row margin--sm" style={{ justifyContent: 'end' }}>
              <Link
                className={clsx(
                  'button button--secondary button--outline button--lg margin--sm',
                  styles.hoverSecondary,
                  styles.smallScreenFlexButton
                )}
                to="https://hexdocs.pm/membrane_live_compositor_plugin/0.9.0/Membrane.LiveCompositor.html">
                <IconContext.Provider value={{ style: { verticalAlign: 'middle' } }}>
                  <FaBook style={{ marginRight: 5 }} />
                  Docs
                </IconContext.Provider>
              </Link>
              <Link
                className={clsx(
                  'button',
                  'button--secondary',
                  'button--outline',
                  'button--lg',
                  'margin--sm',
                  styles.hoverSecondary,
                  styles.smallScreenFlexButton
                )}
                to="https://github.com/membraneframework/membrane_live_compositor_plugin">
                <IconContext.Provider value={{ style: { verticalAlign: 'middle' } }}>
                  <FaGithub style={{ marginRight: 5 }} />
                  Repository
                </IconContext.Provider>
              </Link>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}

function Usage() {
  return (
    <div className="container">
      <Heading as="h2" className="margin-bottom--md text--center text--primary">
        Usage
      </Heading>
      <StandaloneSever />
      <br />
      <MembranePlugin />
    </div>
  );
}

function Licensing() {
  return (
    <div className="container margin-top--lg margin-bottom--md">
      <Heading as="h2" className="margin-bottom--md text--center text--primary">
        Licensing
      </Heading>
      <div className="card container">
        <div className="card__body">
          <p className={styles.grayText}>
            LiveCompositor is licensed under{' '}
            <Link to="https://github.com/software-mansion/live-compositor/blob/master/LICENSE">
              Business Source License 1.1
            </Link>
          </p>
          <p className={styles.grayText}>
            Currently, allowed usage is limited to non-production use cases. If you are interested
            in the production usage contact us (see the section below).
          </p>
          <span className={styles.grayText}>
            What is the goal of those limitations?
            <ul>
              <li>We want to have insight into where and how LiveCompositor is used.</li>
              <li>We want to block third parties from re-packaging and providing it as a SaaS.</li>
              <li>
                In the future, we will add additional grants for free production usage (including
                commercial ones). However, at least for now, we want to keep the ability to decide
                that on a case-by-case basis.
              </li>
            </ul>
          </span>
          <span className={styles.grayText}>
            What is <b>not</b> our goal?
            <ul>
              <li>
                We do not want to vendor-lock or provide predatory per-seat/per-core/per-instance
                pricing. Exact conditions will be discussed case-by-case, but you can expect terms
                you are comfortable with and that do not threaten your business model.
              </li>
              <li>
                We do not want to block use cases already achievable with existing open-source
                tooling. You can expect that in most cases like that we will allow free production
                and commercial use.
              </li>
            </ul>
          </span>
        </div>
      </div>
    </div>
  );
}

function ContactUs() {
  return (
    <div className="container margin-bottom--lg">
      <div className="card container">
        <div className="row" style={{ justifyContent: 'center' }}>
          <SwmLogo width={220} className="margin--lg" style={{ alignSelf: 'center' }} />
          <div className="col">
            <div className="card__header margin-top--md">
              <Heading as="h2">
                <span className="text--primary">Contact</span> us
              </Heading>
            </div>
            <div className="card__body">
              <p className={clsx(styles.grayText, 'margin-bottom--md')}>
                LiveCompositor is developed by Software Mansion - a software company that is
                specialized in building tools for developers. At Software Mansion, we work on
                multiple multimedia projects, like Membrane Framework, Elixir WebRTC, FishJam, and
                more. We also work on custom solutions for clients. Email us at{' '}
                <Link to="mailto:projects@swmansion.com">projects@swmansion.com</Link> or contact us
                via <Link to="https://swmansion.com/contact">this form</Link>.
              </p>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}

export default function Home(): JSX.Element {
  return (
    <>
      <Layout>
        <HomepageHeader />
        <div className={styles.sectionSeparator} />
        <HowItWorks />
        <div className={styles.sectionSeparator} />
        <UseCases />
        <div className={styles.sectionSeparator} />
        <VisionCards />
        <div className={styles.sectionSeparator} />
        <Features />
        <div className={styles.sectionSeparator} />
        <Usage />
        <div className={styles.sectionSeparator} />
        <Licensing />
        <div className={styles.sectionSeparator} />
        <ContactUs />
      </Layout>
    </>
  );
}
