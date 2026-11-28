import { TriangleAlert } from 'lucide-react';
import Link from 'next/link';
import { DownloadButton } from '@/components/download-button';
import { Screenshot } from '@/components/screenshot';
import { hidHideUrl, issuesUrl, tagline, vigemBusUrl } from '@/lib/shared';
import styles from './landing.module.css';

const drivers = [
  {
    name: 'ViGEmBus',
    need: 'Required',
    required: true,
    what: 'Creates the Virtual Controllers. Its author retired it in 2023, so it gets no further updates.',
    url: vigemBusUrl,
  },
  {
    name: 'HidHide',
    need: 'Recommended',
    required: false,
    what: 'Hides Controllers from games. Without it, games may see each Controller twice.',
    url: hidHideUrl,
  },
];

export default function HomePage() {
  return (
    <>
      <section className={`${styles.wrap} ${styles.hero}`}>
        <h1>{tagline}</h1>
        <p>
          heka is a tray app. It creates a Virtual Controller that games see in place of your
          Controller, and Hides the real one so they do not see it twice.
        </p>
        <div className={styles.actions}>
          <DownloadButton
            className={`${styles.button} ${styles.primary}`}
            detailClassName={styles.detail}
          />
          <a className={styles.button} href="#install">
            How to install
          </a>
        </div>
        <ul className={styles.facts}>
          <li>Windows 10 or 11, 64-bit</li>
          <li>Portable zip, no installer</li>
          <li>Free and open source, no telemetry</li>
        </ul>
        <div className={styles.shot}>
          <Screenshot
            name="controller"
            alt="heka's window: a DualSense on Bluetooth, Bridged as an Xbox controller, with its battery level and a drawing of its input"
          />
        </div>
      </section>

      <section className={`${styles.wrap} ${styles.install}`} id="install">
        <h2>Install heka</h2>
        <ol className={styles.steps}>
          <li>
            <h3>Install the drivers</h3>
            <p>Use their official installers, and restart Windows if asked.</p>
            <ul className={styles.drivers}>
              {drivers.map((driver) => (
                <li key={driver.name}>
                  <strong>{driver.name}</strong>
                  <span className={driver.required ? `${styles.tag} ${styles.required}` : styles.tag}>
                    {driver.need}
                  </span>
                  <span className={styles.what}>{driver.what}</span>
                  <a className={styles.button} href={driver.url}>
                    Get {driver.name}
                  </a>
                </li>
              ))}
            </ul>
          </li>
          <li>
            <h3>Download and extract heka</h3>
            <p>Extract the zip to a folder you will keep.</p>
          </li>
          <li>
            <h3>Close other controller tools</h3>
            <p>
              DS4Windows, DualSenseX and Steam Input compete with heka.{' '}
              <Link className={styles.link} href="/docs/conflicts">
                See conflicts
              </Link>
              .
            </p>
          </li>
          <li>
            <h3>Run heka.exe</h3>
            <p>
              It is not code signed, so Windows SmartScreen may warn the first time: choose{' '}
              <b>More info</b>, then <b>Run anyway</b>.
            </p>
          </li>
          <li>
            <h3>Connect your Controller</h3>
            <p>
              It is Bridged automatically, as an Xbox controller until you choose otherwise. To pair
              over Bluetooth, hold <b>PS</b> and <b>Create</b> (DualSense) or <b>PS</b> and{' '}
              <b>Share</b> (DualShock 4) until the light bar flashes, then add it in Windows&apos;
              Bluetooth settings.
            </p>
          </li>
        </ol>
        <div className={styles.note}>
          <TriangleAlert />
          <p>
            <b>DualShock 4 support is experimental.</b> It has never been tried on a real one.{' '}
            <a className={styles.link} href={issuesUrl}>
              Report what works and what does not
            </a>
            .
          </p>
        </div>
        <div className={styles.next}>
          <Link className={styles.button} href="/docs">
            Read the docs
          </Link>
          <Link className={styles.button} href="/docs/troubleshooting">
            Troubleshooting
          </Link>
        </div>
      </section>
    </>
  );
}
